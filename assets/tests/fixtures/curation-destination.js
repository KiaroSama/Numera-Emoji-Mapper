// Real drag regressions for destination lookup at the first settled boundary.
import assert from 'node:assert/strict';
import { expect, test, synth } from '../browser-fixture.js';

const LEVELS = new Set(['INFO', 'WARNING', 'ERROR', 'DEBUG']);
function log(level, message) {
  if (!LEVELS.has(level)) throw new Error('Unknown destination regression log level');
  console.log(`${new Date().toISOString()} [${level}] ${message}`);
}

export async function runForwardDestinationScenarios(panel) {
  for (const scenario of [
    {name: 'the only forward stamp is immediately beside the target', packs: [7, undefined, 2], source: 0, target: 1, expected: 2},
    {name: 'the adjacent forward stamp wins over a later pack', packs: [7, undefined, 2, 3], source: 0, target: 1, expected: 2},
    {name: 'a held neighbor cannot supply the destination', packs: [7, undefined, 9, 2, 3], excluded: [2], source: 0, target: 1, expected: 2},
    {name: 'a preceding settled stamp retains precedence', packs: [1, 7, undefined, 2], source: 1, target: 2, expected: 1},
    {name: 'another carried card cannot supply the destination', packs: [7, undefined, 8, 2, 3], source: 0, target: 1, group: [0, 2], expected: 2},
    {name: 'an entirely unstamped destination stays unstamped', packs: [7, undefined, undefined], source: 0, target: 1, expected: null},
  ]) {
    await test.step(scenario.name, async () => {
      log('INFO', `Checking destination: ${scenario.name}`);
      const items = synth(scenario.packs.length, {packs: scenario.packs, excluded: scenario.excluded});
      const page = await panel.open(items, {clock: true});
      if (scenario.group) {
        await page.click('#selmode');
        for (const index of scenario.group) await page.locator(`#grid .card[data-key="${items[index].key}"] .thumb`).click();
      }
      const before = await page.evaluate('snapshot()');
      const moving = (scenario.group || [scenario.source]).map(index => items[index].key);
      await page.locator(`#grid .card[data-key="${items[scenario.source].key}"]`)
        .dragTo(page.locator(`#grid .card[data-key="${items[scenario.target].key}"]`), {targetPosition: {x: 10, y: 80}});
      const actual = await page.evaluate(keys => keys.map(key => ITEMS.find(item => item.key === key).pack ?? null), moving);
      log('DEBUG', `Expected destination ${scenario.expected}; observed ${JSON.stringify(actual)}`);
      assert.deepEqual(actual, moving.map(() => scenario.expected),
        'a drop must inspect the immediately following settled card before looking farther ahead');
      const after = await page.evaluate('snapshot()');
      await page.click('#undo');
      assert.deepEqual(await page.evaluate('snapshot()'), before, 'Undo restores the original pack stamps and order');
      await page.click('#redo');
      assert.deepEqual(await page.evaluate('snapshot()'), after, 'Redo restores the accepted destination');
      await page.click('#save');
      const packs = new Map(await page.evaluate("__sent('/api/save')[0].body.packs"));
      for (const key of moving) assert.equal(packs.get(key) ?? null, scenario.expected,
        'the explicit Save must carry the same pack shown by the drop');
    });
  }

  await test.step('cancelling after aiming at a forward stamp preserves the complete snapshot', async () => {
    log('INFO', 'Checking cancellation after a forward destination was aimed at');
    const items = synth(4, {packs: [7, undefined, 2, 3]});
    const page = await panel.open(items, {clock: true});
    const before = await page.evaluate('snapshot()');
    const source = page.locator(`#grid .card[data-key="${items[0].key}"]`);
    const target = page.locator(`#grid .card[data-key="${items[1].key}"]`);
    await source.hover();
    await page.mouse.down();
    await target.hover({position: {x: 10, y: 80}});
    await target.hover({position: {x: 10, y: 80}});
    const indicator = page.locator('#dragTarget');
    await expect(indicator).toBeVisible();
    await expect(indicator).toHaveText('Drop -> Pack 2 (3/200)');
    await page.locator('#selCount').hover();
    await page.locator('#selCount').hover();
    await expect(indicator).toBeHidden();
    await expect(indicator).toBeEmpty();
    // Re-entering a valid target must restore the feedback for the same gesture.
    await target.hover({position: {x: 10, y: 80}});
    await target.hover({position: {x: 10, y: 80}});
    await expect(indicator).toBeVisible();
    await expect(indicator).toHaveText('Drop -> Pack 2 (3/200)');
    await page.locator('#selCount').hover();
    await page.locator('#selCount').hover();
    await expect(indicator).toBeHidden();
    await page.mouse.up();
    await expect(indicator).toBeHidden();
    await expect(indicator).toBeEmpty();
    assert.deepEqual(await page.evaluate('snapshot()'), before);
    await page.clock.runFor(500);
    assert.deepEqual(await page.evaluate("__sent('/api/order')"), [], 'a cancelled drag must not queue its preview');
    assert.deepEqual(await page.evaluate("__sent('/api/save')"), []);
  });

  await test.step('entering Holding clears pack feedback before the drop parks the original item', async () => {
    log('INFO', 'Checking destination feedback while a grid drag enters Holding');
    const items = synth(4, {packs: [7, undefined, 2, 3]});
    const page = await panel.open(items, {clock: true});
    const before = await page.evaluate('snapshot()');
    const source = page.locator(`#grid .card[data-key="${items[0].key}"]`);
    const target = page.locator(`#grid .card[data-key="${items[1].key}"]`);
    await source.hover();
    await page.mouse.down();
    await target.hover({position: {x: 10, y: 80}});
    await target.hover({position: {x: 10, y: 80}});
    const indicator = page.locator('#dragTarget');
    await expect(indicator).toBeVisible();
    await expect(indicator).toHaveText('Drop -> Pack 2 (3/200)');
    await page.locator('#holding').hover({position: {x: 15, y: 15}});
    await page.locator('#holding').hover({position: {x: 15, y: 15}});
    await expect(indicator).toBeHidden();
    await expect(indicator).toBeEmpty();
    await page.mouse.up();
    const after = await page.evaluate('snapshot()');
    assert.deepEqual(after.order, before.order, 'Holding must discard the transient grid reorder');
    assert.deepEqual(after.packs, before.packs, 'Holding must retain the original pack stamp');
    assert.deepEqual(after.included, before.included.filter(key => key !== items[0].key));
    await page.click('#undo');
    assert.deepEqual(await page.evaluate('snapshot()'), before);
    await page.click('#redo');
    assert.deepEqual(await page.evaluate('snapshot()'), after);
    await page.click('#save');
    const saved = await page.evaluate("__sent('/api/save')[0].body");
    assert.ok(saved.excluded.includes(items[0].key));
    assert.equal(new Map(saved.packs).get(items[0].key), 7);
  });
}
