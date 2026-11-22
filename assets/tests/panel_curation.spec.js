// Faithful real-browser scenarios; original class/method identity is retained.
import assert from "node:assert/strict";
import { test, expect, synth } from "./browser-fixture.js";
test.use({ catalogSize: 4 });
test.skip(process.env.NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS === '1', 'Explicit browser opt-out');

async function open(panel, items, options) {
  if (items === undefined) panel.reset();
  const page = await panel.open(items, options);
  page.setDefaultTimeout(5000);
  return page;
}

test("test_panel_curation.CurationControls.test_toolbar_does_not_cover_brand_or_save_at_desktop_widths", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  for (const width of [1920, 1200]) {
    await test.step(`width=${width}`, async () => {
      (await page.setViewportSize({"width": width, "height": 900}));
      let brand = (await page.locator("header h1").boundingBox());
      let toolbar = (await page.locator("header .actions").boundingBox());
      let save = (await page.locator("header .hright").boundingBox());
      assert.ok((brand["x"] + brand["width"]) <= toolbar["x"]);
      assert.ok((toolbar["x"] + toolbar["width"]) <= save["x"]);
    });
  }
});

test("test_panel_curation.CurationControls.test_shift_hold_unhold_and_history_restore_the_whole_gesture", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  (await page.click("#selmode"));
  assert.deepEqual((await page.locator(".tick:visible").count()), 0);
  (await page.locator(".card .pick").nth(1).click());
  (await page.locator(".card .pick").nth(4).click({modifiers: ["Shift"]}));
  assert.deepEqual((await page.locator(".card.picked").count()), 4);
  (await page.click("#toHold"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 4);
  assert.deepEqual((await page.locator("#grid .card").count()), 8);
  assert.deepEqual((await page.locator(".packsep .n").first().innerText()), "#1–#2");
  (await page.click("#undo"));
  assert.deepEqual((await page.locator("#grid .card").count()), 12);
  assert.deepEqual((await page.locator(".card.picked").count()), 4);
  (await page.click("#redo"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 4);
  (await page.getByRole("button", {name: "Unhold all", exact: true}).click());
  assert.deepEqual((await page.locator("#grid .card").count()), 12);
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 0);
  (await page.click("#undo"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 4);
  (await page.locator("#holdCards .hcard button").first().click());
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 3);

  await test.step("tray picks have their own undo entry and restore the Shift anchor", async () => {
    const items = synth(5, {excluded: [0, 1, 2, 3]});
    const trayPage = await open(panel, items);
    await trayPage.click("#selmode");
    const tray = trayPage.locator("#holdCards .hcard");
    const beforePick = await trayPage.evaluate("snapshot()");
    await tray.nth(1).locator("img").click();
    const selected = await trayPage.evaluate("snapshot()");
    await trayPage.click("#undo");
    assert.deepEqual(await trayPage.evaluate("snapshot()"), beforePick,
      "Undo must reverse the tray pick, not leave selection mode or unhold cards");
    await trayPage.click("#redo");
    assert.deepEqual(await trayPage.evaluate("snapshot()"), selected);
    await tray.nth(0).locator("img").click();
    await trayPage.click("#undo");
    await tray.nth(3).locator("img").click({modifiers: ["Shift"]});
    assert.deepEqual(await trayPage.evaluate("heldPicked()"), items.slice(1, 4).map(item => item.key),
      "Undo must restore the previous tray anchor before the next Shift click");
  });
});

test("test_panel_curation.CurationControls.test_several_held_emoji_are_picked_and_come_back_in_one_gesture", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(12).fill(1)})));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(0).click());
  (await page.locator(".card .pick").nth(4).click({modifiers: ["Shift"]}));
  (await page.click("#toHold"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 5);
  assert.deepEqual((await page.locator("#holdCards .hcard .pick").count()), 5);
  (await page.locator("#holdCards .hcard .pick").nth(0).click());
  (await page.locator("#holdCards .hcard .pick").nth(3).click({modifiers: ["Shift"]}));
  assert.deepEqual((await page.locator("#holdCards .hcard.picked").count()), 4);
  assert.deepEqual((await page.locator("#selLabel").innerText()), "Selection: 4 picked");
  (await page.locator("#holdCards .hcard.picked button").first().click());
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);
  assert.deepEqual((await page.locator("#grid .card").count()), 11);
});

test("test_panel_curation.CurationControls.test_a_held_emoji_joins_the_pack_it_is_dropped_into", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(0).click());
  (await page.click("#toHold"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);
  let moved = (await page.evaluate(() => {
                const it = ITEMS.find(x => !x.included && !x.isLogo);
                const from = it.pack;
                const other = packStarts().starts.find(s => s.pack !== from);
                // What `dragstart` does on the real path: without it the drop
                // leaves no history entry and Undo pops the hold instead.
                remember();
                holdDragKeys = new Set([it.key]);
                carried.clear(); carried.add(it.key);
                // And what `dragover` does. The pack comes from the card the
                // pointer is ON, so a test that skips this is testing a drop
                // that cannot happen -- nothing was aimed at, so by design
                // nothing is stamped.
                aimAt(other.index);
                ITEMS.splice(ITEMS.indexOf(it), 1);
                ITEMS.splice(other.index + 1, 0, it);
                const accepted = acceptDrop();
                return {accepted, from, into: other.pack, now: it.pack,
                        included: it.included,
                        assigned: assignedPacks().get(it.key),
                        runs: packStarts().starts.map(s => s.pack)};
            }));
  assert.ok(moved["accepted"], "moving between packs is what the panel is for");
  assert.ok(moved["included"], "the emoji must land in the grid, not stay held");
  assert.deepEqual(moved["now"], moved["into"], "it must carry the destination pack");
  assert.deepEqual(moved["assigned"], moved["into"]);
  assert.deepEqual(moved["runs"], [moved["from"], moved["into"]]);
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 0);
  (await page.click("#undo"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1, "undo puts the emoji back in the tray");
  assert.deepEqual((await page.evaluate(() => ITEMS.find(x => !x.included && !x.isLogo).pack)), moved["from"], "undo must roll the pack stamp back as well");
});

test("test_panel_curation.CurationControls.test_a_full_destination_pack_refuses_the_drop_and_points_at_the_tray", async ({ panel }) => {
  let page = (await open(panel, synth(202, {packs: Array(2).fill(1).concat(Array(200).fill(2)), excluded: [0]})));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);
  let refused = (await page.evaluate(() => {
                const it = ITEMS.find(x => !x.included && !x.isLogo);
                const full = packStarts().starts.find(s => s.pack !== it.pack);
                holdDragKeys = new Set([it.key]);
                carried.clear(); carried.add(it.key);
                aimAt(full.index);               // the pointer was over the full pack
                ITEMS.splice(ITEMS.indexOf(it), 1);
                ITEMS.splice(full.index + 1, 0, it);
                let said = null; const real = toast; toast = m => { said = m; };
                const accepted = acceptDrop();
                toast = real;
                return {accepted, said, held: !it.included, into: full.pack, pack: it.pack};
            }));
  assert.ok(!refused["accepted"], "a full pack must take nothing");
  assert.ok(refused["held"], "the emoji stays in the tray");
  assert.deepEqual(refused["pack"], 1, "a refused drop must not re-stamp it");
  assert.ok(refused["said"].includes(`Pack ${refused["into"]} is full (200/200)`));
  assert.ok(refused["said"].includes("Hold one of its emoji first"));

  for (const scenario of [
    {name: "one grid card cannot enter a full pack", destinationSize: 200, group: false, samePack: false, accepted: false},
    {name: "a picked group is refused atomically when one slot remains", destinationSize: 199, group: true, samePack: false, accepted: false},
    {name: "a picked group fills two remaining slots", destinationSize: 198, group: true, samePack: false, accepted: true},
    {name: "one grid card fills the last free slot", destinationSize: 199, group: false, samePack: false, accepted: true},
    {name: "reordering inside a full pack does not consume a new slot", destinationSize: 200, group: false, samePack: true, accepted: true},
    {name: "the global logo reserves a slot in every destination", destinationSize: 199, group: false, samePack: false, accepted: false, logo: "global"},
    {name: "a global logo leaves 199 ordinary slots in the destination", destinationSize: 198, group: false, samePack: false, accepted: true, logo: "global"},
    {name: "an explicit destination logo is counted once", destinationSize: 198, group: false, samePack: false, accepted: true, logo: "per-pack"},
  ]) {
    await test.step(scenario.name, async () => {
      const items = synth(scenario.destinationSize + 2, {logo: !!scenario.logo,
        packs: [1, 1, ...Array(scenario.destinationSize).fill(2)]});
      const ordinary = items.filter(item => !item.isLogo);
      if (scenario.logo === "per-pack") {
        items[0].pack = 1;
        items.splice(3, 0, {...items[0], key: "__logo_pack_2__", pack: 2});
      }
      const gridPage = await open(panel, items, {clock: true});
      await gridPage.evaluate("setZoom(0.4)");
      if (scenario.group) {
        await gridPage.click("#selmode");
        await gridPage.locator("#grid .card .thumb").nth(0).click();
        await gridPage.locator("#grid .card .thumb").nth(1).click();
      }
      const before = await gridPage.evaluate("snapshot()");
      const source = ordinary[scenario.samePack ? 2 : 0].key;
      const target = ordinary[scenario.samePack ? 4 : 2].key;
      await gridPage.locator(`#grid .card[data-key="${source}"]`)
        .dragTo(gridPage.locator(`#grid .card[data-key="${target}"]`));
      const destination = await gridPage.evaluate("ITEMS.filter(item => !item.isLogo && item.included && item.pack === 2).length");
      const capacity = scenario.logo ? 199 : 200;
      assert.equal(destination, scenario.accepted && !scenario.samePack ? capacity : scenario.destinationSize,
        "a drop must never overfill its destination");
      if (scenario.accepted) {
        assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, source), 2);
        assert.notDeepEqual(await gridPage.evaluate("snapshot()"), before);
      } else {
        assert.deepEqual(await gridPage.evaluate("snapshot()"), before,
          "a rejected drag must restore the complete pre-drag state");
        assert.ok((await gridPage.locator("#toast").innerText()).includes("Hold one of its emoji first"));
        await gridPage.clock.runFor(500);
        assert.deepEqual(await gridPage.evaluate("__sent('/api/order')"), [],
          "a refused drag must not enqueue an order");
      }
    });
  }

  for (const scenario of [
    {numbered: 199, logo: false, accepted: false},
    {numbered: 198, logo: false, accepted: true},
    {numbered: 198, logo: true, accepted: false},
    {numbered: 197, logo: true, accepted: true},
  ]) {
    await test.step(`final membership counts an unstamped tail: numbered=${scenario.numbered}, logo=${scenario.logo}`, async () => {
      const items = synth(scenario.numbered + 3, {logo: scenario.logo,
        packs: [1, 1, ...Array(scenario.numbered).fill(2)]});
      const ordinary = items.filter(item => !item.isLogo);
      const source = ordinary[0].key, tail = ordinary.at(-1).key;
      const gridPage = await open(panel, items, {clock: true});
      await gridPage.evaluate("setZoom(0.4)");
      const before = await gridPage.evaluate("snapshot()");
      await gridPage.locator(`#grid .card[data-key="${source}"]`)
        .dragTo(gridPage.locator(`#grid .card[data-key="${tail}"]`), {targetPosition: {x: 2, y: 2}});
      const destination = await gridPage.evaluate(() => {
        const assignments = assignedPacks();
        return ITEMS.filter(item => !item.isLogo && item.included && assignments.get(item.key) === 2).length;
      });
      assert.equal(destination, scenario.numbered + 1 + Number(scenario.accepted),
        "the unnumbered tail must be counted in the run it joins after the drop is stamped");
      if (scenario.accepted) {
        assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, source), 2);
        assert.equal(await gridPage.evaluate(key => assignedPacks().get(key), tail), 2);
        assert.notDeepEqual(await gridPage.evaluate("snapshot()"), before);
      } else {
        assert.deepEqual(await gridPage.evaluate("snapshot()"), before);
        assert.ok((await gridPage.locator("#toast").innerText()).includes("Hold one of its emoji first"));
        await gridPage.clock.runFor(500);
        assert.deepEqual(await gridPage.evaluate("__sent('/api/order')"), []);
      }
    });
  }

  for (const preceding of [200, 199]) {
    await test.step(`vacating a pack boundary retains the preceding pack's capacity: preceding=${preceding}`, async () => {
      const items = synth(preceding + 4, {
        packs: [...Array(preceding).fill(1), 2, undefined, 3, 3]});
      const source = items[preceding].key, tail = items[preceding + 1].key;
      const gridPage = await open(panel, items, {clock: true});
      await gridPage.evaluate("setZoom(0.4)");
      const before = await gridPage.evaluate("snapshot()");
      await gridPage.locator(`#grid .card[data-key="${source}"]`)
        .dragTo(gridPage.locator(`#grid .card[data-key="${items.at(-1).key}"]`));
      if (preceding === 200) {
        assert.deepEqual(await gridPage.evaluate("snapshot()"), before,
          "removing a boundary must not overflow the source side through an unstamped neighbor");
        assert.ok((await gridPage.locator("#toast").innerText()).includes("Pack 1 is full"));
        await gridPage.clock.runFor(500);
        assert.deepEqual(await gridPage.evaluate("__sent('/api/order')"), []);
      } else {
        assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, source), 3);
        assert.equal(await gridPage.evaluate(key => assignedPacks().get(key), tail), 1);
        assert.equal(await gridPage.evaluate("ITEMS.filter(item => item.included && assignedPacks().get(item.key) === 1).length"), 200);
      }
    });
  }

  for (const scenario of [
    {name: "reduce existing overflow", packs: [...Array(201).fill(1), 2, 2], from: 0, to: 201, accepted: true},
    {name: "reorder existing overflow", packs: [...Array(201).fill(1), 2, 2], from: 0, to: 2, accepted: true},
    {name: "leave unrelated overflow unchanged", packs: [...Array(201).fill(1), 2, 2, 3], from: 201, to: 203, accepted: true},
    {name: "refuse worsening existing overflow", packs: [1, 1, ...Array(201).fill(2)], from: 0, to: 2, accepted: false},
  ]) {
    await test.step(scenario.name, async () => {
      const items = synth(scenario.packs.length, {packs: scenario.packs});
      const gridPage = await open(panel, items, {clock: true});
      await gridPage.evaluate("setZoom(0.4)");
      const before = await gridPage.evaluate("snapshot()");
      await gridPage.locator(`#grid .card[data-key="${items[scenario.from].key}"]`)
        .dragTo(gridPage.locator(`#grid .card[data-key="${items[scenario.to].key}"]`));
      if (scenario.accepted) {
        assert.notDeepEqual(await gridPage.evaluate("snapshot()"), before);
        assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, items[scenario.from].key), scenario.packs[scenario.to]);
        if (scenario.from === 0 && scenario.to === 201) {
          assert.equal(await gridPage.evaluate("ITEMS.filter(item => item.included && item.pack === 1).length"), 200);
        }
      } else {
        assert.deepEqual(await gridPage.evaluate("snapshot()"), before);
        await gridPage.clock.runFor(500);
        assert.deepEqual(await gridPage.evaluate("__sent('/api/order')"), []);
      }
    });
  }

  await test.step("a held pick does not consume capacity during a grid-origin drop", async () => {
    const items = synth(202, {packs: [1, 1, 1, ...Array(199).fill(2)], excluded: [1]});
    const gridPage = await open(panel, items, {clock: true});
    await gridPage.evaluate("setZoom(0.4)");
    await gridPage.click("#selmode");
    await gridPage.locator(`#grid .card[data-key="${items[0].key}"] .thumb`).click();
    await gridPage.locator(`#holdCards .hcard[data-key="${items[1].key}"] img`).click();
    await gridPage.locator(`#grid .card[data-key="${items[0].key}"]`)
      .dragTo(gridPage.locator(`#grid .card[data-key="${items[3].key}"]`));
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, items[0].key), 2);
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).included, items[1].key), false);
    assert.equal(await gridPage.locator("#holdCards .hcard").count(), 1);
    assert.equal(await gridPage.evaluate("ITEMS.filter(item => item.included && item.pack === 2).length"), 200);
  });

  for (const destinationSize of [199, 198]) {
    await test.step(`a tray-origin group activates both carried cards: destination=${destinationSize}`, async () => {
      const items = synth(destinationSize + 3, {packs: [1, 1, 1, ...Array(destinationSize).fill(2)], excluded: [0, 1]});
      const trayPage = await open(panel, items, {clock: true});
      await trayPage.evaluate("setZoom(0.4)");
      await trayPage.click("#selmode");
      await trayPage.locator("#holdCards .hcard img").nth(0).click();
      await trayPage.locator("#holdCards .hcard img").nth(1).click();
      const before = await trayPage.evaluate("snapshot()");
      await trayPage.locator(`#holdCards .hcard[data-key="${items[0].key}"]`)
        .dragTo(trayPage.locator(`#grid .card[data-key="${items[3].key}"]`));
      if (destinationSize === 199) {
        assert.deepEqual(await trayPage.evaluate("snapshot()"), before,
          "the entire tray group must stay held when only one slot is available");
        assert.equal(await trayPage.locator("#holdCards .hcard").count(), 2);
        await trayPage.clock.runFor(500);
        assert.deepEqual(await trayPage.evaluate("__sent('/api/order')"), []);
      } else {
        assert.equal(await trayPage.locator("#holdCards .hcard").count(), 0);
        assert.equal(await trayPage.evaluate("ITEMS.filter(item => item.included && item.pack === 2).length"), 200);
      }
    });
  }
});

test("test_panel_curation.CurationControls.test_unhold_all_refuses_a_full_original_pack_without_partial_changes", async ({ panel }) => {
  let items = synth(203, {packs: Array(201).fill(1).concat(Array(2).fill(2)), excluded: [195, 202]});
  let page = (await open(panel, items));
  (await page.getByRole("button", {name: "Unhold all", exact: true}).click());
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 2);
  assert.ok((await page.locator("#toast").innerText()).includes("Pack 1 is full (200/200)"));
  (await page.locator("#holdCards .hcard button").last().click());
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);

  await test.step("a global logo also reserves space when unholding into a later pack", async () => {
    const branded = await open(panel, synth(201, {logo: true,
      packs: [1, ...Array(200).fill(2)], excluded: [1]}));
    await branded.click("#unholdAll");
    assert.equal(await branded.locator("#holdCards .hcard").count(), 1);
    assert.ok((await branded.locator("#toast").innerText()).includes("Pack 2 is full (200/200)"));
  });
});

test("test_panel_curation.CurationControls.test_reset_uses_the_submitted_save_and_is_itself_undoable", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.click("#anim"));
  (await page.click("#bg"));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").first().click());
  (await page.click("#toHold"));
  (await page.click("#save"));
  (await page.waitForFunction("__sent('/api/save').length===1"));
  let expected = (await page.evaluate("snapshot()"));
  (await page.locator(".card .pick").first().click());
  (await page.click("#toHold"));
  (await page.click("#anim"));
  (await page.evaluate("__settle(0,200)"));
  (await page.clock.runFor(50));
  assert.ok((await page.evaluate("selDirty()")));
  let edited = (await page.evaluate("snapshot()"));
  (await page.click("#resetAll"));
  assert.deepEqual((await page.evaluate("snapshot()")), expected);
  (await page.click("#undo"));
  assert.deepEqual((await page.evaluate("snapshot()")), edited);
  (await page.click("#redo"));
  assert.deepEqual((await page.evaluate("snapshot()")), expected);

  await test.step("Reset reprojects held cards before restoring a different zoom", async () => {
    const resetPage = await open(panel, synth(6, {excluded: [0]}));
    const checkpoint = await resetPage.evaluate("snapshot()");
    await resetPage.click("#unholdAll");
    await resetPage.locator("#zoomReset").fill("50%");
    await resetPage.locator("#zoomReset").press("Enter");
    await resetPage.click("#resetAll");
    assert.deepEqual(await resetPage.evaluate("snapshot()"), checkpoint);
    assert.deepEqual(panel.errors, [], "Reset must not look up a row for the card it just held");
    await resetPage.click("#undo");
    assert.equal(await resetPage.evaluate("ITEMS[0].included"), true);
    assert.equal(await resetPage.evaluate("zoom"), 0.5);
  });
});

test("test_panel_curation.CurationControls.test_hold_save_and_reload_use_the_real_server_and_catalog", async ({ panel }) => {
  let page = (await open(panel, undefined));
  (await page.evaluate("window.__net.passthrough=true"));
  (await page.click("#selmode"));
  let held_key = (await page.locator(".card").first().getAttribute("data-key"));
  (await page.locator(".card .pick").first().click());
  (await page.click("#toHold"));
  (await page.click("#save"));
  (await page.waitForFunction("pendingSel===null && !selDirty()"));
  assert.deepEqual(panel.excludedInDb(), new Set([held_key]));
  (await page.reload({waitUntil: "load"}));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);
  assert.deepEqual((await page.locator(`#grid .card[data-key="${held_key}"]`).count()), 0);

  await test.step("explicit Save persists an imported order and reload retains it", async () => {
    await page.evaluate("window.__net.passthrough=true");
    const reversed = [...panel.dbOrder()].reverse();
    const draft = await page.evaluate("snapshot()");
    draft.order = reversed;
    await Import(panel, page, {version: 1, snapshot: draft});
    await expect.poll(() => page.evaluate("ITEMS.filter(item => !item.isLogo).map(item => item.key)"))
      .toEqual(reversed);
    await page.click("#save");
    await expect.poll(() => panel.dbOrder(), {timeout: 5000}).toEqual(reversed);
    await page.waitForFunction("pendingOrder === null && pendingSel === null");
    assert.equal(await page.evaluate("__unload()"), false);
    await page.reload({waitUntil: "load"});
    assert.deepEqual(await page.evaluate("ITEMS.filter(item => !item.isLogo).map(item => item.key)"), reversed);
    assert.deepEqual(panel.excludedInDb(), new Set([held_key]));
  });
});

test("test_panel_curation.CurationControls.test_native_drag_history_and_unhold_restore_the_original_slot", async ({ panel }) => {
  let items = synth(6, {packs: Array(6).fill(1)});
  let page = (await open(panel, items));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(1).click());
  (await page.click("#toHold"));
  let first = page.locator("#grid .card").first();
  (await first.dragTo(page.locator("#grid .card").last()));
  assert.deepEqual((await page.locator("#grid .card").first().getAttribute("data-key")), items[2]["key"]);
  (await page.click("#undo"));
  assert.deepEqual((await page.locator("#grid .card").first().getAttribute("data-key")), items[0]["key"]);
  (await page.click("#redo"));
  (await page.locator("#holdCards .hcard button").click());
  assert.deepEqual((await page.locator("#grid .card").nth(1).getAttribute("data-key")), items[1]["key"]);
});

async function held(panel, page, count = 5) {
  (await page.click("#selmode"));
  (await page.evaluate(n => holdKeys(new Set(ITEMS.filter(x=>!x.isLogo && x.included).slice(0, n).map(x=>x.key))), count));
  (await page.waitForTimeout(200));
  return page.locator("#holdCards .hcard");
}

async function ticks(panel, page) {
  return (await page.evaluate("[...document.querySelectorAll('#holdCards .hcard')].map(c => getComputedStyle(c.querySelector('.pick'), '::after').content)"));
}

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_an_unselected_tick_is_empty_and_only_a_selected_one_shows_a_check", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let cards = (await held(panel, page));
  assert.ok((await ticks(panel, page)).map(t => (t === "none")).every(Boolean), `a tick was drawn with nothing selected: ${(await ticks(panel, page))}`);
  (await cards.nth(1).locator("img").click());
  (await page.waitForTimeout(150));
  let drawn = (await ticks(panel, page)).map((value, index) => [index, value]).filter(([i, t]) => (t !== "none")).map(([i, t]) => i);
  assert.deepEqual(drawn, [1], "exactly the selected card shows a check");
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_clicking_a_cards_face_selects_it_and_shift_takes_the_range", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let cards = (await held(panel, page));
  (await cards.nth(0).locator("img").click());
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("picked.size")), 1);
  (await cards.nth(3).locator("img").click({modifiers: ["Shift"]}));
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("picked.size")), 4, "Shift must take the whole inclusive range");
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_one_click_on_the_box_toggles_once_not_twice", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let cards = (await held(panel, page));
  (await cards.nth(0).locator(".pick").click());
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("picked.size")), 1);
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_unhold_does_not_disturb_the_selection", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let cards = (await held(panel, page));
  (await cards.nth(0).locator("img").click());
  (await cards.nth(2).locator("img").click());
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("picked.size")), 2);
  (await page.locator("#holdCards .hcard button").nth(4).click());
  (await page.waitForTimeout(250));
  assert.deepEqual((await page.evaluate("picked.size")), 2, "unholding an unpicked card changed the selection");
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_outside_selection_mode_a_click_selects_nothing", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let cards = (await held(panel, page));
  (await page.click("#selmode"));
  (await page.waitForTimeout(150));
  (await cards.nth(0).locator("img").click());
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("picked.size")), 0);
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_a_drag_from_the_tray_carries_every_selected_emoji", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  let cards = (await held(panel, page, 3));
  (await cards.nth(0).locator("img").click());
  (await cards.nth(2).locator("img").click({modifiers: ["Shift"]}));
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("heldPicked().length")), 3);
  let transfer = (await page.evaluateHandle(() => new DataTransfer()));
  (await cards.nth(0).dispatchEvent("dragstart", {"dataTransfer": transfer}));
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("holdDragKeys.size")), 3, "the drag must carry the whole selection, not one card");
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_dragging_an_unselected_held_card_carries_only_itself", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  let cards = (await held(panel, page, 3));
  (await cards.nth(0).locator("img").click());
  (await page.waitForTimeout(150));
  let transfer = (await page.evaluateHandle(() => new DataTransfer()));
  (await cards.nth(2).dispatchEvent("dragstart", {"dataTransfer": transfer}));
  (await page.waitForTimeout(150));
  assert.deepEqual((await page.evaluate("holdDragKeys.size")), 1);
});

test("test_panel_curation.TheHoldTrayIsSelectableByHand.test_a_card_dropped_from_the_tray_into_the_grid_leaves_the_tray", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  let cards = (await held(panel, page, 1));
  let key = (await cards.nth(0).getAttribute("data-key"));
  let transfer = (await page.evaluateHandle(() => new DataTransfer()));
  (await cards.nth(0).dispatchEvent("dragstart", {"dataTransfer": transfer}));
  let target = page.locator("#grid .card:not(.logo)").nth(3);
  (await target.dispatchEvent("dragover", {"dataTransfer": transfer}));
  (await target.dispatchEvent("drop", {"dataTransfer": transfer}));
  assert.ok((await page.evaluate(`ITEMS.find(x => x.key === ${JSON.stringify(key)}).included`)));
  assert.deepEqual((await page.locator(`#holdCards .hcard[data-key="${key}"]`).count()), 0, "a card back in the grid must leave the tray at once");
  assert.deepEqual((await page.locator("#holdCount").innerText()), "0");
});

async function FullTray(panel) {
  let page = (await open(panel, synth(600)));
  (await page.click("#none"));
  assert.deepEqual((await page.evaluate("heldOrder().length")), 600);
  return page;
}

async function ToEnd(panel, page) {
  (await page.evaluate("const h = document.getElementById('holding'); h.scrollLeft = h.scrollWidth"));
  let last = (await page.evaluate("heldOrder()[599]"));
  (await page.waitForFunction(`!!document.querySelector('#holdCards .hcard[data-key="${last}"]')`));
  return last;
}

test("test_panel_curation.ALargeTrayIsWindowed.test_a_large_tray_mounts_only_what_is_in_view", async ({ panel }) => {
  let page = (await FullTray(panel));
  assert.ok((await page.locator("#holdCards .hcard").count()) <= 60);
  (await ToEnd(panel, page));
  assert.ok((await page.locator("#holdCards .hcard").count()) <= 60);
});

test("test_panel_curation.ALargeTrayIsWindowed.test_shift_range_picks_across_unmounted_tray_cards", async ({ panel }) => {
  let page = (await FullTray(panel));
  (await page.click("#selmode"));
  (await page.locator("#holdCards .hcard").first().click());
  let last = (await ToEnd(panel, page));
  (await page.locator(`#holdCards .hcard[data-key="${last}"]`).click({modifiers: ["Shift"]}));
  assert.deepEqual((await page.evaluate("heldPicked().length")), 600);
});

test("test_panel_curation.ALargeTrayIsWindowed.test_picking_a_run_touches_each_card_once", async ({ panel }) => {
  let page = (await open(panel, synth(400)));
  (await page.click("#selmode"));
  let calls = (await page.evaluate("(() => {\n            let n = 0; const real = markPicked;\n            markPicked = function(){ n++; return real.apply(this, arguments); };\n            paintFrom = 0; paintTo = true; strokeSpan = []; strokeBase = new Set();\n            const t = performance.now();\n            for (let j = 0; j < 300; j++) applyStroke(j);\n            const ms = performance.now() - t;\n            markPicked = real;\n            return [n, ms]; })()"));
  console.log(("PANEL_PERF " + JSON.stringify({"scenario": "stroke-300", "mark_calls": calls[0], "ms": (Math.round(calls[1] * 10) / 10)})));
  assert.deepEqual(calls[0], 300, "each card is marked once as the stroke reaches it");
});

async function Import(panel, page, doc) {
  (await page.setInputFiles("#draftFile", [{"name": "numera-emoji-mapper-draft.json", "mimeType": "application/json", "buffer": Buffer.from(JSON.stringify(doc), "utf8")}]));
}

test("test_panel_curation.AnExportedDraftComesBack.test_a_draft_is_applied_without_saving_and_can_be_undone", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  let before = (await page.evaluate("ITEMS.map(x => x.key)"));
  let snap = (await page.evaluate("snapshot()"));
  snap["order"] = [...snap["order"]].reverse().concat(["x:gone"]);
  (await Import(panel, page, {"version": 1, "snapshot": snap}));
  (await page.waitForFunction("ITEMS[0].key !== %s".replace("%s", JSON.stringify(before[0]))));
  assert.deepEqual((await page.evaluate("ITEMS.map(x => x.key)")), [...before].reverse());
  assert.equal((await page.evaluate("pendingOrder")) ?? null, null, "an import must not post by itself");
  assert.ok(await page.evaluate("__unload()"), "an imported order must remain protected before explicit Save");
  assert.ok(await page.evaluate("document.getElementById('save').classList.contains('dirty')"),
    "the Save control must show the unsaved imported arrangement");
  assert.ok((await page.evaluate("document.getElementById('toast').textContent")).includes("1 not in this catalog"));
  (await page.click("#undo"));
  assert.deepEqual((await page.evaluate("ITEMS.map(x => x.key)")), before);
});

test("test_panel_curation.AnExportedDraftComesBack.test_an_unknown_version_is_refused", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  let before = (await page.evaluate("ITEMS.map(x => x.key)"));
  (await Import(panel, page, {"version": 2, "snapshot": {"order": [...before].reverse()}}));
  (await page.waitForFunction("document.getElementById('toast').textContent.includes('Not a draft')"));
  assert.deepEqual((await page.evaluate("ITEMS.map(x => x.key)")), before);
});

async function holdFirst(panel, page) {
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(0).click());
  (await page.click("#toHold"));
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1);
}

test("test_panel_curation.ADroppedEmojiJoinsThePackItWasAimedAt.test_a_drop_on_a_packs_first_card_joins_that_pack_not_the_one_above", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  (await holdFirst(panel, page));
  let moved = (await page.evaluate(() => {
                const it = ITEMS.find(x => !x.included && !x.isLogo);
                const head = packStarts().starts.find(s => s.pack === 2).index;
                remember();
                holdDragKeys = new Set([it.key]);
                carried.clear(); carried.add(it.key);
                aimAt(head);            // the pointer is ON pack 2's first card
                moveCarried(head);      // dropped on its left half: before it
                const accepted = acceptDrop();
                return {accepted, pack: it.pack, at: ITEMS.indexOf(it),
                        head: packStarts().starts.find(s => s.pack === 2).index,
                        runs: packStarts().starts.map(s => s.pack)};
            }));
  assert.ok(moved["accepted"]);
  assert.deepEqual(moved["pack"], 2, "aimed at pack 2, so it joins pack 2");
  assert.deepEqual(moved["at"], moved["head"]);
  assert.deepEqual(moved["runs"], [1, 2], "no run may be cut by a drop");
});

test("test_panel_curation.ADroppedEmojiJoinsThePackItWasAimedAt.test_a_grid_to_grid_move_re_stamps_and_does_not_split_the_pack", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  let moved = (await page.evaluate(() => {
                const it = ITEMS.find(x => x.pack === 1 && x.included && !x.isLogo);
                const mid = packStarts().starts.find(s => s.pack === 2).index + 2;
                dragKey = it.key; dragSnap = snapshot(); aimedPack = undefined;
                carried.clear(); carried.add(it.key);
                aimAt(mid); moveCarried(mid);
                const accepted = acceptDrop();
                commitDrag(); endDrag(true);
                return {accepted, pack: it.pack,
                        runs: packStarts().starts.map(s => s.pack)};
            }));
  assert.ok(moved["accepted"]);
  assert.deepEqual(moved["pack"], 2, "a grid drag must re-stamp too");
  assert.deepEqual(moved["runs"], [1, 2], "the destination pack was split by the dropped card");
});

test("test_panel_curation.ADroppedEmojiJoinsThePackItWasAimedAt.test_a_drop_that_aimed_at_nothing_stamps_nothing", async ({ panel }) => {
  let page = (await open(panel, synth(12, {packs: Array(6).fill(1).concat(Array(6).fill(2))})));
  (await holdFirst(panel, page));
  let after = (await page.evaluate(() => {
                const it = ITEMS.find(x => !x.included && !x.isLogo);
                const was = it.pack;
                holdDragKeys = new Set([it.key]);
                carried.clear(); carried.add(it.key);
                aimedPack = undefined;           // no dragover ever fired
                const accepted = acceptDrop();
                return {accepted, was, now: it.pack};
            }));
  assert.ok(after["accepted"], "the emoji still comes out of the tray");
  assert.deepEqual(after["now"], after["was"]);
});

test("test_panel_curation.ADroppedEmojiJoinsThePackItWasAimedAt.test_a_drop_into_a_run_with_no_pack_number_invents_no_number", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  let after = (await page.evaluate(() => {
                const it = ITEMS.find(x => x.included && !x.isLogo);
                it.pack = 7;                     // a stamp from somewhere else
                dragKey = it.key; dragSnap = snapshot(); aimedPack = undefined;
                carried.clear(); carried.add(it.key);
                aimAt(6); moveCarried(6);
                const accepted = acceptDrop();
                endDrag(true);
                return {accepted, aimed: aimedPack, now: it.pack ?? null};
            }));
  assert.ok(after["accepted"]);
  assert.equal(after["aimed"] ?? null, null, "an unnumbered run has no number to aim at");
  assert.equal(after["now"] ?? null, null, "the emoji joins the run, it does not keep a number");
});

test("test_panel_curation.TheGridIsSelectableByClick.test_clicking_a_cards_face_selects_it_and_shift_takes_the_range", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  (await page.click("#selmode"));
  (await page.locator(".card .thumb").nth(1).click());
  assert.deepEqual((await page.evaluate("picked.size")), 1);
  (await page.locator(".card .thumb").nth(4).click({modifiers: ["Shift"]}));
  assert.deepEqual((await page.evaluate("picked.size")), 4, "Shift must take the whole inclusive range");
});

test("test_panel_curation.TheGridIsSelectableByClick.test_a_click_on_the_box_still_toggles_exactly_once", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(2).click());
  assert.deepEqual((await page.evaluate("picked.size")), 1);
  (await page.locator(".card .pick").nth(2).click());
  assert.deepEqual((await page.evaluate("picked.size")), 0, "and once back off again");
});

test("test_panel_curation.TheGridIsSelectableByClick.test_outside_selection_mode_a_click_still_toggles_what_ships", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  (await page.locator(".card .thumb").nth(0).click());
  assert.deepEqual((await page.locator("#holdCards .hcard").count()), 1, "an unticked emoji goes to the tray, as it always has");
  assert.deepEqual((await page.evaluate("picked.size")), 0);
});

test("test_panel_curation.TheGridIsSelectableByClick.test_the_brand_logo_is_never_picked", async ({ panel }) => {
  let page = (await open(panel, synth(6, {logo: true})));
  (await page.click("#selmode"));
  (await page.locator(".card.logo").click());
  assert.deepEqual((await page.evaluate("picked.size")), 0);
});

async function ring(panel, page, selector) {
  return (await page.evaluate(s => {const n = document.querySelector(s); return n ? {anim: getComputedStyle(n, '::before').animationName,  play: n.getAnimations({subtree: true}).map(a => a.playState)} : null}, selector));
}

test("test_panel_curation.APickedCardWearsAMovingRing.test_only_a_picked_card_carries_the_ring_and_it_runs", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  (await page.click("#selmode"));
  (await page.locator(".card .thumb").nth(0).click());
  let marked = (await ring(panel, page, ".card.picked"));
  let plain = (await ring(panel, page, ".card:not(.picked)"));
  assert.deepEqual(marked["anim"], "pickspin");
  assert.ok(marked["play"].includes("running"), "a still ring is not a moving one");
  assert.deepEqual(plain["anim"], "none");
});

test("test_panel_curation.APickedCardWearsAMovingRing.test_a_picked_held_card_carries_the_same_ring", async ({ panel }) => {
  let page = (await open(panel, synth(12)));
  (await page.click("#selmode"));
  (await page.locator(".card .pick").nth(0).click());
  (await page.click("#toHold"));
  (await page.locator("#holdCards .hcard img").click());
  (await page.waitForTimeout(150));
  assert.deepEqual((await ring(panel, page, ".hcard.picked"))["anim"], "pickspin");
});
