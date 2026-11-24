// Real-page regression scenarios, retaining each original Python class/method identity.
import { test, expect, synth, DENY_STORAGE, sleep } from './browser-fixture.js';
import { runGalleryFreezeScenarios } from './fixtures/gallery-motion.js';
import { PER_SET } from './source-fixture.js';

test.skip(process.env.NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS === '1', 'Explicit browser opt-out');
const id = (group, name) => `test_panel_browser.${group}.${name}`;

test(id('F01OrderSavesAreRevisioned', 'test_F01_an_old_acknowledgement_does_not_clear_a_newer_arrangement'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await page.evaluate('__reorder(0, 5)');
  await page.clock.runFor(500);
  expect(await page.evaluate("__sent('/api/order')")).toHaveLength(1);
  await page.evaluate('__reorder(1, 7)');
  const newer = await page.evaluate('pendingOrder');
  await page.evaluate('__settle(0, 200)');
  await page.clock.runFor(50);
  expect(await page.evaluate('pendingOrder'), 'an older acknowledgement cleared newer unsaved work').toEqual(newer);
  expect(await page.evaluate('__unload()'), 'the tab was free to close on an arrangement never saved').toBe(true);
  await page.clock.runFor(2000);
  const sent = await page.evaluate("__sent('/api/order')");
  expect(sent, 'the newer arrangement was never sent').toHaveLength(2);
  expect(sent[1].body.order).toEqual(newer);
  await page.evaluate('__settle(1, 200)');
  await page.clock.runFor(50);
  expect(await page.evaluate('pendingOrder')).toBeNull();
  expect(await page.evaluate('__unload()'), 'nothing is unsaved now').toBe(false);
});

test(id('F01OrderSavesAreRevisioned', 'test_F01_a_failed_save_does_not_overwrite_a_newer_arrangement'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await page.evaluate('__reorder(0, 5)');
  await page.clock.runFor(500);
  const stale = (await page.evaluate("__sent('/api/order')"))[0].body.order;
  await page.evaluate('__reorder(1, 7)');
  const newer = await page.evaluate('pendingOrder');
  expect(stale).not.toEqual(newer);
  await page.evaluate('__settle(0, 500)');
  await page.clock.runFor(50);
  expect(await page.evaluate('pendingOrder'), 'a failed old snapshot replaced the newer arrangement').toEqual(newer);
  await page.clock.runFor(3000);
  expect((await page.evaluate("__sent('/api/order')")).at(-1).body.order, 'the retry pushed the stale arrangement').toEqual(newer);
});

test(id('F01OrderSavesAreRevisioned', 'test_F01_only_one_order_request_is_ever_in_flight'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await page.evaluate('__reorder(0, 5)');
  await page.clock.runFor(450);
  await page.evaluate('(()=>{ flushOrder(pendingOrder); flushOrder(pendingOrder); })()');
  expect(await page.evaluate("__sent('/api/order')"), 'two concurrent saves race for the same catalog rows').toHaveLength(1);
  await page.evaluate('__reorder(1, 7)');
  await page.evaluate('(()=>{ flushOrder(pendingOrder); })()');
  await page.clock.runFor(1000);
  expect(await page.evaluate("__sent('/api/order')")).toHaveLength(1);
  await page.evaluate('__settle(0, 200)');
  await page.clock.runFor(50);
  const sent = await page.evaluate("__sent('/api/order')");
  expect(sent, 'the queued arrangement never followed').toHaveLength(2);
  expect(sent[1].body.order).toEqual(await page.evaluate('pendingOrder'));
});

test(id('F01TheCatalogKeepsTheNewestOrder', 'test_F01_a_failed_save_still_lands_the_newest_arrangement'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  const posted = [];
  page.on('request', request => { if (request.url().includes('/api/order')) posted.push(request.postData()); });
  await page.evaluate('__reorder(0, 5)');
  await page.clock.runFor(500);
  const stale = (await page.evaluate("__sent('/api/order')"))[0].body.order;
  await page.evaluate('__reorder(1, 7)');
  const newer = await page.evaluate('pendingOrder');
  await page.evaluate('__net.passthrough = true');
  await page.evaluate('__settle(0, 500)');
  let order = panel.dbOrder();
  for (const deadline = Date.now() + 20_000; JSON.stringify(order) !== JSON.stringify(newer) && Date.now() < deadline;) {
    await page.clock.runFor(1100);
    await sleep(100);
    order = panel.dbOrder();
  }
  expect(order, 'the catalog kept an arrangement the owner had replaced').toEqual(newer);
  expect(posted.map(body => body ? JSON.parse(body).order : null), 'a stale snapshot was pushed at the catalog').not.toContainEqual(stale);
  await page.reload({ waitUntil: 'load' });
  expect(await page.evaluate('ITEMS.filter(x=>!x.isLogo).map(x=>x.key)'), 'the reloaded page disagrees with the catalog').toEqual(newer);
});

test(id('F01TheCatalogKeepsTheNewestOrder', 'test_F01_a_real_drag_reaches_the_catalog'), async ({ panel }) => {
  const page = await panel.open();
  await page.evaluate('__net.passthrough = true');
  const before = panel.dbOrder();
  await page.locator('#grid .card').nth(0).dragTo(page.locator('#grid .card').nth(5));
  const moved = await page.evaluate('ITEMS.map(x=>x.key)');
  expect(moved, 'the drag did not move anything').not.toEqual(before);
  await expect.poll(() => panel.dbOrder(), { timeout: 20_000, intervals: [100] }).toEqual(moved);
});

const alerting = page => page.evaluate("document.getElementById('alert').classList.contains('show')");
async function failedSave(page) {
  await page.locator('#grid .card').nth(1).click();
  await page.click('#save');
  const saves = await page.evaluate("__sent('/api/save')");
  expect(saves, 'Save did not reach the panel').toHaveLength(1);
  await page.evaluate(`__settle(${saves[0].i}, 500)`);
  await page.clock.runFor(50);
  expect(await alerting(page), 'a refused Save must not be silent').toBe(true);
  return saves[0].body.excluded;
}

test(id('F02SelectionSavesSurviveAFailure', 'test_F02_a_successful_ping_does_not_dismiss_a_failed_save'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await failedSave(page);
  await page.clock.runFor(6000);
  expect(await alerting(page), 'liveness was reported as persistence').toBe(true);
});

test(id('F02SelectionSavesSurviveAFailure', 'test_F02_the_tab_is_protected_until_the_save_lands'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await failedSave(page);
  expect(await page.evaluate('__unload()'), 'the tab could close on a selection that never saved').toBe(true);
  await page.clock.runFor(6000);
  const pending = (await page.evaluate("__sent('/api/save')")).filter(call => !call.settled);
  expect(pending, 'exactly one retry, and it is in flight').toHaveLength(1);
  await page.evaluate(`__settle(${pending[0].i}, 200)`);
  await page.clock.runFor(50);
  expect(await alerting(page), 'the warning outlived the problem').toBe(false);
  expect(await page.evaluate('__unload()')).toBe(false);
});

test(id('F02SelectionSavesSurviveAFailure', 'test_F02_a_refused_save_is_retried_with_what_Save_asked_for'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  const asked = await failedSave(page);
  await page.locator('#grid .card').nth(2).click();
  await page.clock.runFor(6000);
  const saves = await page.evaluate("__sent('/api/save')");
  expect(saves.length, 'a refused Save was simply forgotten').toBeGreaterThan(1);
  expect(saves.at(-1).body.excluded, 'the retry saved something Save was never asked to save').toEqual(asked);
});

test(id('F02SelectionSavesSurviveAFailure', 'test_F02_an_order_success_does_not_dismiss_a_failed_save'), async ({ panel }) => {
  const page = await panel.open(undefined, { clock: true });
  await failedSave(page);
  await page.evaluate('__reorder(0, 2)');
  await page.clock.runFor(500);
  const order = (await page.evaluate("__sent('/api/order')")).at(-1);
  await page.evaluate(`__settle(${order.i}, 200)`);
  await page.clock.runFor(50);
  expect(await alerting(page), "a saved ORDER cleared a failed SELECTION's warning").toBe(true);
});

const RUNS = [1, 1, 2, 2, 1, 1];
test(id('F03EveryRunHasItsOwnSeparator', 'test_F03_a_repeated_pack_gets_a_separator_of_its_own'), async ({ panel }) => {
  const page = await panel.open(synth(6, { packs: RUNS }));
  expect(await page.evaluate('rows.filter(r=>r.sep).length'), 'the fixture must model three runs').toBe(3);
  expect(await page.evaluate("document.querySelectorAll('#grid .packsep').length"), 'two runs shared one marker, so the grid lost a boundary').toBe(3);
});

test(id('F03EveryRunHasItsOwnSeparator', 'test_F03_each_marker_sits_at_the_head_of_its_own_run'), async ({ panel }) => {
  const page = await panel.open(synth(6, { packs: RUNS }));
  const laidOut = await page.evaluate(`[...grid.children]
    .filter(n => n.classList.contains('card') || n.classList.contains('packsep'))
    .map(n => n.classList.contains('packsep') ? n.querySelector('span').textContent : '#' + n.querySelector('.pos').textContent)`);
  expect(laidOut).toEqual(['Pack 1', '#1', '#2', 'Pack 2', '#3', '#4', 'Pack 1', '#5', '#6']);
});

test(id('F03EveryRunHasItsOwnSeparator', 'test_F03_reordering_keeps_one_marker_per_run'), async ({ panel }) => {
  const page = await panel.open(synth(6, { packs: RUNS }));
  await page.evaluate('__reorder(5, 0)');
  expect(await page.evaluate('rows.filter(r=>r.sep).length'), 'the row heights and the markers disagree after a move')
    .toBe(await page.evaluate("document.querySelectorAll('#grid .packsep').length"));
});

test(id('F03EveryRunHasItsOwnSeparator', 'test_F03_a_run_that_goes_away_takes_its_marker_with_it'), async ({ panel }) => {
  const page = await panel.open(synth(6, { packs: RUNS }));
  await page.evaluate('(()=>{ for(const i of [2,3]){ ITEMS[i].included = false; setCard(ITEMS[i]); } relayout(); updateCount(); })()');
  expect(await page.evaluate("document.querySelectorAll('#grid .packsep').length"), 'a marker for a run that no longer exists stayed in the grid')
    .toBe(await page.evaluate('rows.filter(r=>r.sep).length'));
});

async function anchored(page, before) {
  const after = await page.evaluate('__anchor()');
  if (after.atEnd && after.top?.start <= before.idx) return;
  expect(after.top, 'the grid scrolled past its own rows').not.toBeNull();
  if (after.cols === before.cols) expect(after.top.start, 'the zoom landed somewhere else in the grid').toBe(before.idx);
  else expect(after.top.start <= before.idx && before.idx < after.top.end,
    `the anchor ${before.idx} fell out of the top row ${JSON.stringify(after.top)}`).toBe(true);
}

test(id('F04ZoomKeepsItsAnchor', 'test_F04_the_anchor_survives_a_zoom_anywhere_in_the_grid'), async ({ panel }) => {
  const page = await panel.open(synth(1000));
  const places = { top: 'scrollTo(0, 0)', middle: 'scrollTo(0, document.documentElement.scrollHeight / 2)', bottom: 'scrollTo(0, document.documentElement.scrollHeight)' };
  for (const [where, scroll] of Object.entries(places)) for (const button of ['#zoomIn', '#zoomOut']) {
    await test.step(`${where} ${button}`, async () => {
      await page.evaluate('setZoom(1)');
      await page.evaluate(scroll);
      const before = await page.evaluate('__anchor()');
      expect(before.idx).toBeGreaterThanOrEqual(0);
      await page.click(button);
      await anchored(page, before);
    });
  }
});

test(id('F04ZoomKeepsItsAnchor', 'test_the_zoom_level_survives_a_reload'), async ({ panel }) => {
  const page = await panel.open(synth(40));
  await page.click('#zoomIn');
  const zoomed = await page.evaluate('zoom');
  expect(zoomed).toBeGreaterThan(1);
  await page.reload({ waitUntil: 'load' });
  expect(await page.evaluate('zoom')).toBe(zoomed);
  expect(await page.evaluate("document.getElementById('zoomReset').value")).toBe(`${Math.round(zoomed * 100)}%`);
});

test(id('F04ZoomKeepsItsAnchor', 'test_F04_a_zoom_round_trip_comes_back_to_the_same_item'), async ({ panel }) => {
  const page = await panel.open(synth(1000, { packs: Array.from({ length: 1000 }, (_, i) => 1 + Math.floor(i / 150)) }));
  await page.evaluate('scrollTo(0, document.documentElement.scrollHeight)');
  const before = await page.evaluate('__anchor()');
  for (let i = 0; i < 3; i++) await page.click('#zoomIn');
  await page.dblclick('#zoomReset');
  expect(await page.evaluate('zoom')).toBe(1);
  await anchored(page, before);
});

async function barrier(page, action) {
  await page.evaluate(`(()=>{
    window.__tick = 0;
    window.__probe = new IntersectionObserver(() => { window.__tick++; }, {root: null, rootMargin: '0px'});
    document.querySelectorAll('#grid img[data-anim]').forEach(n => __probe.observe(n));
  })()`);
  await page.waitForFunction('__tick > 0', null, { timeout: 10_000 });
  await page.evaluate(`(()=>{ window.__tick = 0; ${action} })()`);
  await page.waitForFunction('__tick > 0', null, { timeout: 10_000 });
}

test(id('F05AFreezeIsNeverUndone', 'test_F05_what_you_can_see_still_animates'), async ({ panel }) => {
  const page = await panel.open(synth(300, { fmt: 'animated' }));
  await page.waitForFunction('__animating() > 0', null, { timeout: 10_000 });
  expect(panel.errors).toEqual([]);

  const card = page.locator('#grid .card').first();
  const thumb = card.locator('.thumb');
  const playing = () => page.evaluate('__animating()');
  await test.step('Drag and window blur pause animation until they end', async () => {
    const dataTransfer = await page.evaluateHandle(() => new DataTransfer());
    await card.dispatchEvent('dragstart', { dataTransfer });
    expect(await playing()).toBe(0);
    await page.evaluate('allocate()');
    expect(await playing()).toBe(0);
    await card.dispatchEvent('dragend', { dataTransfer });
    await dataTransfer.dispose();
    await page.waitForFunction('__animating() > 0');
    await page.evaluate(() => window.dispatchEvent(new Event('blur')));
    expect(await playing()).toBe(0);
    await page.evaluate('allocate()');
    expect(await playing()).toBe(0);
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await page.waitForFunction('__animating() > 0');
  });

  await test.step('Hover only previews one card and obeys the master switch', async () => {
    await page.click('#animHover');
    await expect(page.locator('#animAll')).toBeDisabled();
    expect(await playing()).toBe(0);
    await thumb.hover();
    expect(await playing()).toBe(1);
    expect(await thumb.locator('img').evaluate(img => img.getAttribute('src') === img.dataset.anim)).toBe(true);
    await page.mouse.move(1, 1);
    expect(await playing()).toBe(0);
    await expect(thumb).toHaveAttribute('tabindex', '0');
    await thumb.focus();
    expect(await playing()).toBe(1);
    await page.locator('#animHover').focus();
    expect(await playing()).toBe(0);
    await page.click('#anim');
    await thumb.hover();
    expect(await playing()).toBe(0);
  });
});

test(id('F05AFreezeIsNeverUndone', 'test_F05_a_hidden_page_stays_frozen_when_new_cards_arrive'), async ({ panel }) => {
  const page = await panel.open(synth(300, { fmt: 'animated' }));
  await page.waitForFunction('__animating() > 0', null, { timeout: 10_000 });
  await page.evaluate(`(()=>{ Object.defineProperty(document, 'hidden', {configurable: true, get: () => true}); document.dispatchEvent(new Event('visibilitychange')); })()`);
  expect(await page.evaluate('__animating()'), 'the freeze did not happen').toBe(0);
  await barrier(page, 'scrollBy(0, innerHeight * 3);');
  expect(await page.evaluate('__animating()'), 'an observer callback restarted a hidden page').toBe(0);
  expect(panel.errors).toEqual([]);
  await runGalleryFreezeScenarios(panel);
});

test(id('F05AFreezeIsNeverUndone', 'test_F05_a_switched_off_grid_stays_frozen_when_new_cards_arrive'), async ({ panel }) => {
  const page = await panel.open(synth(300, { fmt: 'animated' }));
  await page.click('#anim');
  expect(await page.evaluate('__animating()')).toBe(0);
  await barrier(page, 'scrollBy(0, innerHeight * 3);');
  expect(await page.evaluate('__animating()'), 'an observer callback overrode the master switch').toBe(0);
});

test(id('F05AFreezeIsNeverUndone', 'test_F05_a_scrolling_grid_stays_frozen_until_it_settles'), async ({ panel }) => {
  const page = await panel.open(synth(300, { fmt: 'animated' }));
  await page.waitForFunction('__animating() > 0', null, { timeout: 10_000 });
  await barrier(page, '__keepScrolling(4000);');
  const state = await page.evaluate('({thawing: scrollThaw !== null, playing: __animating()})');
  expect(state.thawing, 'the freeze window closed before the observer fired; this machine is too slow for this assertion to mean anything').toBe(true);
  expect(state.playing, 'an observer callback thawed a grid that was still scrolling').toBe(0);
  await page.waitForFunction('__animating() > 0', null, { timeout: 10_000 });
});

test(id('F05AFreezeIsNeverUndone', 'test_F05_hover_under_reduced_motion_obeys_the_master_switch'), async ({ panel }) => {
  const page = await panel.open(synth(12, { fmt: 'video' }), { reducedMotion: 'reduce', init: "localStorage.setItem('videoReal','1')" });
  await page.click('#anim');
  await page.locator('#grid .thumb').nth(0).hover();
  expect(await page.evaluate('__plays'), 'hover played a video the owner had switched off').toBe(0);
  await page.click('#anim');
  await page.locator('#grid .thumb').nth(1).hover();
  expect(await page.evaluate('__plays'), 'hover is the only way to play a video under reduced motion').toBeGreaterThan(0);
});

test(id('F05AFreezeIsNeverUndone', 'test_F05_hover_moves_a_video_preview_under_reduced_motion'), async ({ panel }) => {
  const page = await panel.open(synth(12, { fmt: 'video' }), { reducedMotion: 'reduce' });
  const moving = n => n.getAttribute('src') === n.dataset.anim;
  await page.locator('#grid .thumb').nth(1).hover();
  expect(await page.locator('#grid .fmt-video img[data-anim]').nth(1).evaluate(moving)).toBe(true);
  await page.mouse.move(1, 1);
  expect(await page.locator('#grid .fmt-video img[data-anim]').nth(1).evaluate(moving)).toBe(false);
});

test(id('VideoCardsArePreviews', 'test_video_cards_are_previews_by_default'), async ({ panel }) => {
  const page = await panel.open(synth(12, { fmt: 'video' }));
  expect(await page.locator('#grid video').count()).toBe(0);
  expect(await page.locator('#grid .fmt-video img[data-anim]').count()).toBeGreaterThan(0);
  await page.click('#videoReal');
  expect(await page.locator('#grid video').count()).toBeGreaterThan(0);
  expect(await page.locator('#grid .fmt-video img').count()).toBe(0);
  await page.keyboard.press('Control+z');
  expect(await page.locator('#grid video').count()).toBe(0);
  expect(await page.getAttribute('#videoReal', 'aria-pressed')).toBe('false');
});

async function packs(page) {
  const runs = Math.max(1, await page.evaluate('rows.filter(r=>r.sep).length'));
  const warn = await page.evaluate("document.getElementById('capWarn').textContent");
  const shown = await page.evaluate("getComputedStyle(document.getElementById('capWarn')).display") !== 'none';
  const said = warn.match(/(\d+) packs/);
  return [runs, said && shown ? Number(said[1]) : 1, warn];
}

test(id('F06ThePackCountMatchesTheGrid', 'test_F06_the_count_is_the_number_of_runs_the_grid_draws'), async ({ panel }) => {
  const per = PER_SET;
  for (const ordinary of [per - 2, per - 1, per, 2 * per - 3, 2 * per - 2, 2 * per - 1]) await test.step(`ordinary=${ordinary}`, async () => {
    const page = await panel.open(synth(ordinary, { logo: true }));
    const [runs, said, warn] = await packs(page);
    expect(said, `the grid drew ${runs} packs, the header said ${said}: ${warn}`).toBe(runs);
  });
});

test(id('F06ThePackCountMatchesTheGrid', 'test_F06_without_a_logo_the_whole_set_is_available'), async ({ panel }) => {
  const page = await panel.open(synth(PER_SET));
  expect((await packs(page)).slice(0, 2), 'a logo that is not there took a slot').toEqual([1, 1]);
});

test(id('F06ThePackCountMatchesTheGrid', 'test_F06_an_excluded_card_takes_no_slot'), async ({ panel }) => {
  const page = await panel.open(synth(PER_SET, { logo: true, excluded: [0, 1, 2] }));
  expect((await packs(page)).slice(0, 2), 'unticked cards were counted').toEqual([1, 1]);
});

test(id('F06ThePackCountMatchesTheGrid', 'test_F06_live_membership_is_reported_as_an_estimate'), async ({ panel }) => {
  const page = await panel.open(synth(6, { packs: [1, 1, 2, 2, 3, 3] }));
  const [, , warn] = await packs(page);
  expect(warn.toLowerCase(), `a guess presented as fact: ${warn}`).toContain('estimate');
});

test(id('F07DeniedStorageStillBoots', 'test_F07_denied_site_data_does_not_abort_setup'), async ({ panel }) => {
  const page = await panel.open(undefined, { init: DENY_STORAGE });
  expect(panel.errors).toEqual([]);
  expect(await page.evaluate("document.getElementById('zoomReset').value")).toBe('100%');
  expect(await page.evaluate("document.getElementById('animLabel').textContent")).toBe('Animation: On');
  expect(await page.evaluate("document.getElementById('bg').textContent")).toBe('Backdrop: Checker');
  await page.click('#zoomIn');
  expect(await page.evaluate('zoom'), 'zoom is dead without storage').toBeGreaterThan(1);
  await page.click('#anim');
  expect(await page.evaluate("document.getElementById('animLabel').textContent")).toBe('Animation: Off');
  await page.click('#bg');
  expect(await page.evaluate("document.getElementById('bg').textContent")).toBe('Backdrop: Light');
  const before = await page.evaluate("document.getElementById('selCount').textContent");
  await page.locator('#grid .card').nth(0).click();
  expect(await page.evaluate("document.getElementById('selCount').textContent")).not.toBe(before);
  await page.click('#undo');
  expect(await page.evaluate("document.getElementById('selCount').textContent")).toBe(before);
  expect(panel.errors).toEqual([]);
});

test(id('F07DeniedStorageStillBoots', 'test_F07_a_malformed_stored_preference_is_ignored'), async ({ panel }) => {
  const page = await panel.open(undefined, { init: "localStorage.setItem('panelZoom', 'banana'); localStorage.setItem('emojiBg', 'wat'); localStorage.setItem('animOn', 'maybe');" });
  expect(panel.errors).toEqual([]);
  expect(await page.evaluate('zoom')).toBe(1);
  expect(await page.evaluate("document.getElementById('bg').textContent")).toBe('Backdrop: Checker');
  expect(await page.evaluate("document.getElementById('animLabel').textContent")).toBe('Animation: On');
});
