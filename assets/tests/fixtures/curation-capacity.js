// Capacity overflow and unhold-capacity browser scenario runners for panel_curation.spec.js.
import assert from "node:assert/strict";
import { test, synth } from "../browser-fixture.js";

export async function runDropCapacityScenarios(panel, open) {
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
      // Both ends of this capacity gesture must be in the mounted grid window.
      const gridPage = await open(panel, items, {clock: true, viewport: {width: 1200, height: 1800}});
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
      const gridPage = await open(panel, items, {clock: true, viewport: {width: 1200, height: 1800}});
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
      const gridPage = await open(panel, items, {clock: true, viewport: {width: 1200, height: 1800}});
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
    const beforeDrag = await gridPage.evaluate("snapshot()");
    await gridPage.locator(`#grid .card[data-key="${items[0].key}"]`)
      .dragTo(gridPage.locator(`#grid .card[data-key="${items[3].key}"]`));
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, items[0].key), 2);
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).included, items[1].key), false);
    assert.equal(await gridPage.locator("#holdCards .hcard").count(), 1);
    assert.equal(await gridPage.evaluate("ITEMS.filter(item => item.included && item.pack === 2).length"), 200);
    const afterDrop = await gridPage.evaluate("snapshot()");
    await gridPage.locator(`#holdCards .hcard[data-key="${items[1].key}"] button`).click();
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).pack, items[1].key), 1,
      "a grid drag must preserve the held card's original pack for its later Unhold");
    assert.equal(await gridPage.evaluate(key => ITEMS.find(item => item.key === key).included, items[1].key), true);
    assert.equal(await gridPage.evaluate("ITEMS.filter(item => item.included && item.pack === 2).length"), 200);
    const afterUnhold = await gridPage.evaluate("snapshot()");
    await gridPage.click("#undo");
    assert.deepEqual(await gridPage.evaluate("snapshot()"), afterDrop);
    await gridPage.click("#undo");
    assert.deepEqual(await gridPage.evaluate("snapshot()"), beforeDrag);
    await gridPage.click("#redo");
    assert.deepEqual(await gridPage.evaluate("snapshot()"), afterDrop);
    await gridPage.click("#redo");
    assert.deepEqual(await gridPage.evaluate("snapshot()"), afterUnhold);
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
}

export async function runUnholdCapacityScenarios(panel, open) {
  await test.step("a global logo also reserves space when unholding into a later pack", async () => {
    const branded = await open(panel, synth(201, {logo: true,
      packs: [1, ...Array(200).fill(2)], excluded: [1]}));
    await branded.click("#unholdAll");
    assert.equal(await branded.locator("#holdCards .hcard").count(), 1);
    assert.ok((await branded.locator("#toast").innerText()).includes("Pack 2 is full (200/200)"));
  });

  for (const scenario of [
    {name: "a held stamp must not merge two automatic packs", heldPacks: [1], included: 400, accepted: false},
    {name: "one stamp cannot turn 200 candidates into 201 members", heldPacks: [1], included: 200, accepted: false},
    {name: "one stamp fills the 200th ordinary slot", heldPacks: [1], included: 199, accepted: true, counts: [[1, 200]]},
    {name: "the global logo reserves its slot after Unhold changes run mode", heldPacks: [1], included: 199, logo: "global", accepted: false},
    {name: "a global logo permits 199 ordinary members", heldPacks: [1], included: 198, logo: "global", accepted: true, counts: [[1, 199]]},
    {name: "an explicit logo reserves its stamped slot", heldPacks: [1], included: 199, logo: "explicit", accepted: false},
    {name: "an explicit logo permits 199 ordinary members", heldPacks: [1], included: 198, logo: "explicit", accepted: true, counts: [[1, 199]]},
    {name: "two restored stamps must reject their combined overflow", heldPacks: [1, 1], included: 199, accepted: false},
    {name: "two restored stamps fill two remaining slots", heldPacks: [1, 1], included: 198, accepted: true, counts: [[1, 200]]},
    {name: "distinct restored boundaries retain separate capacity", heldPacks: [1, 2], included: 199, accepted: true, counts: [[1, 1], [2, 200]]},
    {name: "all restored boundaries are validated atomically", heldPacks: [1, 2], included: 200, accepted: false},
    {name: "an unstamped Unhold still splits automatic packs", heldPacks: [undefined], included: 200, accepted: true, counts: [["new:0", 200], ["new:1", 1]]},
    {name: "automatic splitting also reserves a global logo", heldPacks: [undefined], included: 199, logo: "global", accepted: true, counts: [["new:0", 199], ["new:1", 1]]},
  ]) {
    await test.step(scenario.name, async () => {
      const heldCount = scenario.heldPacks.length;
      const model = synth(scenario.included + heldCount, {logo: !!scenario.logo,
        packs: scenario.heldPacks, excluded: Array.from({length: heldCount}, (_, index) => index)});
      if (scenario.logo === "explicit") model[0].pack = 1;
      const unholdPage = await open(panel, model, {clock: true});
      await unholdPage.click("#bg");
      await unholdPage.click("#undo");
      const before = await unholdPage.evaluate("snapshot()");
      const history = await unholdPage.evaluate("[past.length, future.length]");
      const runs = await unholdPage.evaluate("packStarts().starts.map(run => run.pack)");
      if (heldCount === 1) await unholdPage.locator("#holdCards .hcard button").click();
      else await unholdPage.click("#unholdAll");
      if (!scenario.accepted) {
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), before,
          "a rejected Unhold must preserve order, membership, origins, selection and view state");
        assert.deepEqual(await unholdPage.evaluate("[past.length, future.length]"), history,
          "a rejected Unhold must preserve both Undo and Redo history");
        assert.ok((await unholdPage.locator("#toast").innerText()).includes("is full (200/200)"));
        await unholdPage.clock.runFor(500);
        assert.deepEqual(await unholdPage.evaluate("__sent('/api/order')"), []);
      } else {
        assert.equal(await unholdPage.locator("#holdCards .hcard").count(), 0);
        assert.deepEqual(await unholdPage.evaluate(() => {
          const assignments = assignedPacks(), counts = new Map();
          for (const item of ITEMS.filter(item => !item.isLogo && item.included)) {
            const pack = assignments.get(item.key);
            counts.set(pack, (counts.get(pack) || 0) + 1);
          }
          return [...counts];
        }), scenario.counts);
        const after = await unholdPage.evaluate("snapshot()");
        await unholdPage.click("#undo");
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), before);
        assert.deepEqual(await unholdPage.evaluate("packStarts().starts.map(run => run.pack)"), runs,
          "Undo must restore automatic or stamped runs together with inclusion");
        await unholdPage.click("#redo");
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), after);
      }
    });
  }

  for (const grow of [false, true]) {
    await test.step(`Unhold with an existing overflow: grows=${grow}`, async () => {
      const model = synth(204, {packs: [...Array(201).fill(1), grow ? 1 : 2, 2, 2], excluded: [201]});
      const unholdPage = await open(panel, model, {clock: true});
      const before = await unholdPage.evaluate("snapshot()");
      const history = await unholdPage.evaluate("[past.length, future.length]");
      await unholdPage.click("#unholdAll");
      assert.equal(await unholdPage.evaluate("ITEMS.filter(item => item.included && item.pack === 1).length"), 201);
      if (grow) {
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), before);
        assert.deepEqual(await unholdPage.evaluate("[past.length, future.length]"), history);
        await unholdPage.clock.runFor(500);
        assert.deepEqual(await unholdPage.evaluate("__sent('/api/order')"), []);
      } else {
        assert.equal(await unholdPage.evaluate("ITEMS.filter(item => item.included && item.pack === 2).length"), 3);
        const after = await unholdPage.evaluate("snapshot()");
        await unholdPage.click("#undo");
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), before);
        await unholdPage.click("#redo");
        assert.deepEqual(await unholdPage.evaluate("snapshot()"), after);
      }
    });
  }

  await test.step("restoring a held boundary can reduce an existing overflow", async () => {
    const model = synth(202, {packs: [1, ...Array(100), 2]});
    const unholdPage = await open(panel, model, {clock: true});
    const boundary = model[101].key;
    const original = await unholdPage.evaluate("snapshot()");
    await unholdPage.evaluate(key => holdKeys(new Set([key])), boundary);
    assert.equal(await unholdPage.evaluate("ITEMS.filter(item => item.included && assignedPacks().get(item.key) === 1).length"), 201);
    const held = await unholdPage.evaluate("snapshot()");
    await unholdPage.locator("#holdCards .hcard button").click();
    assert.deepEqual(await unholdPage.evaluate("snapshot()"), original,
      "the same restored placement used for capacity validation must be committed");
    await unholdPage.click("#undo");
    assert.deepEqual(await unholdPage.evaluate("snapshot()"), held);
    await unholdPage.click("#redo");
    assert.deepEqual(await unholdPage.evaluate("snapshot()"), original);
  });
}
