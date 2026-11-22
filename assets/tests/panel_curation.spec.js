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

// Exhaust every four-card stamp layout and nonempty held subset. The expected
// round trip is the captured original model, independent of placement helpers.
function unholdRunMatrix(){
  const base=snapshot(), ordinary=ITEMS.filter(item=>!item.isLogo).map(item=>item.key);
  const bad=[];let cases=0, failures=0;
  for(let layout=0;layout<81;layout++){
    let value=layout;
    const packs=base.packs.filter(([key])=>!ordinary.includes(key));
    for(const key of ordinary){
      const stamp=value%3;value=Math.floor(value/3);
      if(stamp)packs.push([key,stamp]);
    }
    for(let subset=1;subset<16;subset++){
      applySnapshot({...base,packs,holds:[],runs:[]},false);
      past=[];future=[];pastTopSig=null;updateHistoryButtons();
      const before=JSON.stringify(snapshot());
      const keys=ordinary.filter((key,index)=>subset&(1<<index));
      holdKeys(new Set(keys));
      const held=JSON.stringify(snapshot());
      const accepted=unholdKeys(keys), after=JSON.stringify(snapshot());
      let reason=!accepted?'refused':after!==before?'round trip':null;
      if(accepted){
        undo();if(JSON.stringify(snapshot())!==held)reason=reason||'Undo';
        redo();if(JSON.stringify(snapshot())!==after)reason=reason||'Redo';
      }
      cases++;
      if(reason){failures++;if(bad.length<10)bad.push({layout,subset,reason});}
    }
  }
  return {cases,failures,bad};
}

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

  await test.step("Unhold stays in the later run of a repeated pack", async () => {
    const model=synth(4,{packs:[1,2,1,1]});
    const runPage=await open(panel,model,{clock:true});
    const before=await runPage.evaluate("snapshot()");
    await runPage.click("#selmode");
    await runPage.locator(`#grid .card[data-key="${model[2].key}"] .thumb`).click();
    await runPage.click("#toHold");
    await runPage.locator("#holdCards .hcard button").click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),before.order);
    assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),before.included);
    assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.pack!=null).map(item=>[item.key,item.pack])"),before.packs);
  });

  for(const logo of ["none","global","explicit"]){
    await test.step(`all four-card Hold/Unhold layouts and subsets: logo=${logo}`,async()=>{
      const model=synth(4,{logo:logo!=="none"});
      if(logo==="explicit")model[0].pack=1;
      const matrixPage=await open(panel,model,{clock:true});
      const started=Date.now();
      const result=await matrixPage.evaluate(unholdRunMatrix);
      console.info(`${new Date().toISOString()} [INFO] Unhold run matrix ${JSON.stringify({logo,...result,wall_ms:Date.now()-started})}`);
      assert.equal(result.cases,1215);
      assert.equal(result.failures,0,JSON.stringify(result.bad));
    });
  }

  await test.step("a moved held marker retains its run through draft import and Reset",async()=>{
    const model=synth(7,{packs:[1,1,2,1,1,3,3]});
    const runPage=await open(panel,model,{clock:true});
    const key=model[3].key;
    await runPage.evaluate(key=>holdKeys(new Set([key])),key);
    await runPage.click("#save");
    await runPage.waitForFunction("__sent('/api/save').length===1");
    await runPage.evaluate("__settle(0,200)");
    await runPage.clock.runFor(50);
    const saved=await runPage.evaluate("snapshot()");
    await runPage.click("#selmode");
    await runPage.locator(`#grid .card[data-key="${model[0].key}"] .thumb`).click();
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] img`).click();
    await runPage.locator(`#grid .card[data-key="${model[0].key}"]`)
      .dragTo(runPage.locator(`#grid .card[data-key="${model[5].key}"]`));
    const moved=await runPage.evaluate("snapshot()");
    const expected=moved.order.filter(item=>item!==key);
    expected.splice(expected.indexOf(model[4].key),0,key);
    await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:moved}));
    const imported=await runPage.evaluate("snapshot()");
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),expected,
      "the moved held marker must return before the surviving member of its original run");
    const restored=await runPage.evaluate("snapshot()");
    await runPage.click("#undo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),imported);
    await runPage.click("#redo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),restored);
    await runPage.click("#resetAll");
    assert.deepEqual(await runPage.evaluate("snapshot()"),saved);
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
  });

  for(const missing of ["head","all members"]){
    await test.step(`a draft restores a run with missing ${missing} anchors`,async()=>{
      const original=synth(6,{packs:[1,2,1,1,1,3]});
      const heldIndex=missing==="head"?3:2;
      const absent=missing==="head"?[2]:[3,4];
      const model=original.filter((item,index)=>!absent.includes(index))
        .map(item=>({...item,included:item.key!==original[heldIndex].key}));
      const runPage=await open(panel,model,{clock:true});
      const draft=await runPage.evaluate("snapshot()");
      draft.order=original.map(item=>item.key);
      draft.included=original.filter((item,index)=>index!==heldIndex).map(item=>item.key);
      draft.packs=original.map(item=>[item.key,item.pack]);
      draft.holds=[[original[heldIndex].key,{index:heldIndex,pack:1,slot:heldIndex-2,anchor:original[2].key}]];
      draft.runs=[[original[heldIndex].key,{first:original[2].key,last:original[4].key,
        before:original[1].key,after:original[5].key,slot:heldIndex-2}]];
      await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:draft}));
      const before=await runPage.evaluate("snapshot()");
      await runPage.locator("#holdCards .hcard button").click();
      const expected=missing==="head"?[0,1,4,3,5]:[0,1,2,5];
      assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),expected.map(index=>original[index].key));
      const after=await runPage.evaluate("snapshot()");
      await runPage.click("#undo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),before);
      await runPage.click("#redo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),after);
    });
  }

  await test.step("legacy drafts and bulk inclusion keep run metadata coherent",async()=>{
    const model=synth(4,{logo:true,packs:[1,2,1,1]});
    const runPage=await open(panel,model,{clock:true});
    const before=await runPage.evaluate("snapshot()");
    await runPage.click("#none");
    await runPage.click("#unholdAll");
    assert.deepEqual(await runPage.evaluate("snapshot()"),before);
    await runPage.click("#none");
    await runPage.click("#all");
    assert.deepEqual(await runPage.evaluate("snapshot()"),before);
    const key=model[3].key;
    await runPage.evaluate(key=>holdKeys(new Set([key])),key);
    const legacy=await runPage.evaluate("snapshot()");
    delete legacy.runs;
    await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:legacy}));
    await runPage.locator("#holdCards .hcard button").click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
  });
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

  const model=synth(3,{excluded:[0]}),k0=model[0].key,k1=model[1].key;
  for(const [name,field,value] of [
    ["numeric included","included",1],
    ["object included","included",{}],
    ["non-string included member","included",[k1,1]],
    ["object packs","packs",{}],
    ["null pack entry","packs",[null]],
    ["object pack number","packs",[[k1,{}]]],
    ["string holds","holds","bad"],
    ["null hold entry","holds",[null]],
    ["scalar hold record","holds",[[k0,1]]],
    ["string hold slot","holds",[[k0,{index:0,pack:1,slot:"0",anchor:k0}]]],
    ["negative hold index","holds",[[k0,{index:-1,pack:1,slot:0,anchor:k0}]]],
    ["unsupported hold pack","holds",[[k0,{index:0,pack:"unsupported",slot:0,anchor:k0}]]],
  ]){
    await test.step(`malformed ${name} is refused atomically`,async()=>{
      const draftPage=await open(panel,model,{clock:true});
      await draftPage.click("#bg");
      await draftPage.click("#undo");
      const state=await draftPage.evaluate("snapshot()");
      const history=await draftPage.evaluate("[past,future]");
      const draft={version:1,snapshot:{...state,[field]:value}};
      await Import(panel,draftPage,draft);
      await draftPage.waitForFunction("document.getElementById('toast').textContent.includes('Not a draft')");
      assert.deepEqual(await draftPage.evaluate("snapshot()"),state);
      assert.deepEqual(await draftPage.evaluate("[past,future]"),history);
      assert.equal(await draftPage.locator("#draftFile").inputValue(),"");
      assert.deepEqual(panel.errors,[]);
      await draftPage.clock.runFor(500);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/order')"),[]);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/save')"),[]);
    });
  }

  for(const optional of ["exported","omitted","null"]){
    await test.step(`valid ${optional} optional draft fields remain supported`,async()=>{
      const draftPage=await open(panel,model,{clock:true});
      const state=await draftPage.evaluate("snapshot()");
      const data=optional==="exported"?state:{order:state.order};
      if(optional==="null")Object.assign(data,{included:null,packs:null,holds:null});
      await Import(panel,draftPage,{version:1,snapshot:data});
      await draftPage.waitForFunction("document.getElementById('toast').textContent.startsWith('Imported:')");
      assert.equal(await draftPage.locator("#draftFile").inputValue(),"");
      if(optional==="exported"){
        assert.equal(state.holds[0][1].pack,"new:0","generated automatic pack identities remain valid");
        assert.deepEqual(await draftPage.evaluate("snapshot()"),state);
      }
      else assert.deepEqual(await draftPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),[]);
      await draftPage.clock.runFor(500);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/order')"),[]);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/save')"),[]);
    });
  }

  await test.step("valid drafts reconcile vanished and newly added catalog keys",async()=>{
    const draftPage=await open(panel,model,{clock:true});
    const state=await draftPage.evaluate("snapshot()");
    const draft={...state,order:[model[2].key,"x:gone",k0],included:[model[2].key,"x:gone"]};
    await Import(panel,draftPage,{version:1,snapshot:draft});
    await draftPage.waitForFunction("document.getElementById('toast').textContent.startsWith('Imported:')");
    assert.deepEqual(await draftPage.evaluate("ITEMS.map(item=>item.key)"),[model[2].key,k0,k1]);
    assert.deepEqual(await draftPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),[model[2].key,k1]);
    await draftPage.clock.runFor(500);
    assert.deepEqual(await draftPage.evaluate("__sent('/api/order')"),[]);
    assert.deepEqual(await draftPage.evaluate("__sent('/api/save')"),[]);
    await draftPage.click("#undo");
    assert.deepEqual(await draftPage.evaluate("snapshot()"),state);
  });

  await test.step("duplicate draft keys retain the existing last-occurrence order",async()=>{
    const draftPage=await open(panel,model,{clock:true});
    const state=await draftPage.evaluate("snapshot()");
    const draft={...state,order:[model[2].key,k0,model[2].key]};
    await Import(panel,draftPage,{version:1,snapshot:draft});
    await draftPage.waitForFunction("document.getElementById('toast').textContent.startsWith('Imported:')");
    assert.deepEqual(await draftPage.evaluate("ITEMS.map(item=>item.key)"),[k0,model[2].key,k1]);
    assert.equal(await draftPage.evaluate("new Set(ITEMS.map(item=>item.key)).size"),3);
    await draftPage.clock.runFor(500);
    assert.deepEqual(await draftPage.evaluate("__sent('/api/order')"),[]);
    assert.deepEqual(await draftPage.evaluate("__sent('/api/save')"),[]);
    await draftPage.click("#undo");
    assert.deepEqual(await draftPage.evaluate("snapshot()"),state);
  });

  for(const hint of ["non-list","invalid entry","mixed entries"]){
    await test.step(`optional run hints safely sanitize ${hint}`,async()=>{
      const draftPage=await open(panel,model,{clock:true});
      const state=await draftPage.evaluate("snapshot()");
      const runs=hint==="non-list"?"bad":hint==="invalid entry"?[[k0,{slot:"0"}]]:
        [...state.runs,null,[k1,1]];
      await Import(panel,draftPage,{version:1,snapshot:{...state,runs}});
      await draftPage.waitForFunction("document.getElementById('toast').textContent.startsWith('Imported:')");
      assert.deepEqual(await draftPage.evaluate("snapshot()"),{...state,runs:hint==="mixed entries"?state.runs:[]});
      assert.equal(await draftPage.locator("#draftFile").inputValue(),"");
      assert.deepEqual(panel.errors,[]);
      await draftPage.clock.runFor(500);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/order')"),[]);
      assert.deepEqual(await draftPage.evaluate("__sent('/api/save')"),[]);
    });
  }
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
