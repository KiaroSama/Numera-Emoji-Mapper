// Faithful real-browser scenarios; original class/method identity is retained.
import assert from "node:assert/strict";
import { test, synth } from "./browser-fixture.js";
test.use({ catalogSize: 4 });
test.skip(process.env.NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS === '1', 'Explicit browser opt-out');

async function open(panel, items, options) { return panel.open(items, options); }
async function sent(page, kind) { return page.evaluate(`window.__sent(${JSON.stringify(kind)})`); }

test("test_panel_queues.RefusalsBelongToTheSubmittedBody.test_a_newer_order_outlives_an_older_permanent_refusal", async ({ panel }) => {
  let page = (await open(panel, undefined, {clock: true}));
  (await page.evaluate("__reorder(0, 3)"));
  (await page.clock.runFor(450));
  (await page.evaluate("__reorder(0, 2)"));
  let newer = (await page.evaluate("pendingOrder"));
  (await page.clock.runFor(450));
  (await page.evaluate("__settle(0, 400)"));
  (await page.clock.runFor(1000));
  let requests = (await sent(page, "/api/order"));
  assert.deepEqual((requests).length, 2, "old refusal blocked newer valid work");
  assert.deepEqual(requests[1]["body"]["order"], newer);
  (await page.evaluate("__settle(1, 200)"));
  (await page.clock.runFor(50));
  assert.equal((await page.evaluate("pendingOrder")) ?? null, null);
});

test("test_panel_queues.RefusalsBelongToTheSubmittedBody.test_new_bodies_survive_old_refusals_and_keep_transient_retries", async ({ panel }) => {
  for (const kind of ["order", "save"]) {
    for (const rejection of [400, 409]) {
      await test.step(`queue=${kind}, status=${rejection}`, async () => {
        let page = (await open(panel, undefined, {clock: true}));
        let edit = ((kind === "order") ? "__reorder(0,3)" : "__toggle(0);__save()");
        let newer = ((kind === "order") ? "__reorder(0,2)" : "__toggle(1);__save()");
        (await page.evaluate(edit));
        (await page.clock.runFor(450));
        (await page.evaluate(newer));
        (await page.clock.runFor(450));
        (await page.evaluate(`__settle(0,${rejection})`));
        (await page.clock.runFor(500));
        assert.deepEqual(((await sent(page, `/api/${kind}`))).length, 2);
        (await page.evaluate("__settle(1,503)"));
        (await page.clock.runFor(1100));
        let requests = (await sent(page, `/api/${kind}`));
        assert.deepEqual((requests).length, 3, "new work inherited the old stuck flag");
        assert.deepEqual(requests[1]["body"], requests[2]["body"]);
        (await page.evaluate("__settle(2,200)"));
        (await page.clock.runFor(50));
        let pending = ((kind === "order") ? "pendingOrder" : "pendingSel");
        assert.equal((await page.evaluate(pending)) ?? null, null);
        (await page.close());
      });
    }
  }
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_an_edit_with_no_save_is_dirty_and_blocks_unload", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  assert.ok(!(await page.evaluate("window.__dirty()")), "clean on load");
  (await page.evaluate("window.__toggle(1)"));
  assert.ok((await page.evaluate("window.__dirty()")));
  assert.ok((await page.evaluate("window.__dirtyShown()")), "not shown to me");
  assert.ok((await page.evaluate("window.__unload()")), "the tab would have closed on an unsaved tick");
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_an_edit_made_DURING_a_save_survives_its_acknowledgement", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  let submitted = (await sent(page, "/api/save"))[0]["body"]["excluded"];
  assert.deepEqual((submitted).length, 1);
  (await page.evaluate("window.__toggle(2)"));
  (await page.evaluate("window.__settle(0, 200)"));
  (await page.waitForFunction("window.__sent('/api/save')[0].settled === true"));
  assert.deepEqual(((await page.evaluate("window.__excluded()"))).length, 2);
  assert.ok((await page.evaluate("window.__dirty()")), "an older acknowledgement cleared newer unsaved work");
  assert.ok((await page.evaluate("window.__unload()")));

  await test.step("an older order acknowledgement leaves a newer imported draft unsaved", async () => {
    const importedPage = await open(panel, synth(6), {clock: true});
    await importedPage.evaluate("__reorder(0,3)");
    await importedPage.clock.runFor(450);
    await importedPage.evaluate(() => {
      const draft = snapshot();
      draft.order.reverse();
      importDraft(JSON.stringify({version: 1, snapshot: draft}));
      __settle(0, 200);
    });
    await importedPage.clock.runFor(20);
    assert.equal(await importedPage.evaluate("pendingOrder"), null);
    assert.ok(await importedPage.evaluate("__unload() && __dirtyShown()"),
      "retiring an old order must not clear the newer imported arrangement");
    assert.equal((await sent(importedPage, "/api/order")).length, 1,
      "a draft still waits for explicit Save");
  });
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_saving_the_newer_state_finally_clears_it", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  (await page.evaluate("window.__toggle(2)"));
  (await page.evaluate("window.__settle(0, 200)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 2"));
  (await page.evaluate("window.__settle(1, 200)"));
  (await page.waitForFunction("window.__dirty() === false"));
  assert.ok(!(await page.evaluate("window.__unload()")));

  await test.step("Save supersedes an in-flight order when the import restores the original order", async () => {
    const importedPage = await open(panel, synth(6), {clock: true});
    const original = await importedPage.evaluate("snapshot()");
    await importedPage.evaluate("__reorder(0,3)");
    await importedPage.clock.runFor(450);
    await importedPage.evaluate(draft => {
      importDraft(JSON.stringify({version: 1, snapshot: draft}));
      __save();
      __settle(1, 200);
    }, original);
    await importedPage.clock.runFor(20);
    assert.ok(await importedPage.evaluate("__unload() && __dirtyShown()"));
    assert.ok((await importedPage.locator("#toast").innerText()).includes("still unsaved"),
      "a selection acknowledgement must not claim the in-flight arrangement is saved");
    await importedPage.evaluate("__settle(0,200)");
    await importedPage.clock.runFor(20);
    const requests = await sent(importedPage, "/api/order");
    assert.equal(requests.length, 2);
    assert.deepEqual(requests[1].body.order, original.order,
      "the older in-flight order must be followed by the explicitly saved import");
    await importedPage.evaluate("__settle(2,200)");
    await importedPage.clock.runFor(20);
    assert.ok(!(await importedPage.evaluate("__unload() || __dirtyShown()")));
  });
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_undoing_back_to_the_acknowledged_state_is_clean_again", async ({ panel }) => {
  let page = (await open(panel, synth(6)));
  (await page.evaluate("window.__toggle(1)"));
  assert.ok((await page.evaluate("window.__dirty()")));
  (await page.evaluate("window.__toggle(1)"));
  assert.ok(!(await page.evaluate("window.__dirty()")));
  assert.ok(!(await page.evaluate("window.__unload()")));
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_a_failed_save_leaves_it_dirty_and_a_retry_clears_it", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  (await page.evaluate("window.__settle(0, 503)"));
  (await page.waitForFunction("window.__sent('/api/save')[0].settled === true"));
  assert.ok((await page.evaluate("window.__dirty()")));
  (await page.clock.runFor(2000));
  (await page.waitForFunction("window.__sent('/api/save').length === 2"));
  (await page.evaluate("window.__settle(1, 200)"));
  (await page.waitForFunction("window.__dirty() === false"));
});

test("test_panel_queues.UnsavedSelectionIsProtected.test_a_selection_really_reaches_the_catalog", async ({ panel }) => {
  let page = (await open(panel, undefined));
  (await page.evaluate("window.__net.passthrough = true"));
  (await page.evaluate("window.__toggle(1)"));
  let key = (await page.evaluate("ITEMS[1].key"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__dirty() === false", null, {timeout: 15000}));
  assert.ok(panel.excludedInDb().has(key));
});

test("test_panel_queues.APermanentRefusalIsNotRetried.test_a_400_on_the_order_queue_stops_resubmitting", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__reorder(0, 3)"));
  (await page.clock.runFor(500));
  (await page.waitForFunction("window.__sent('/api/order').length === 1"));
  (await page.evaluate("window.__settle(0, 400)"));
  (await page.waitForFunction("window.__sent('/api/order')[0].settled === true"));
  (await page.clock.runFor(60000));
  assert.deepEqual(((await sent(page, "/api/order"))).length, 1, "a refusal retrying cannot fix was sent again");
});

test("test_panel_queues.APermanentRefusalIsNotRetried.test_a_409_on_the_save_queue_stops_resubmitting", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  (await page.evaluate("window.__settle(0, 409)"));
  (await page.waitForFunction("window.__sent('/api/save')[0].settled === true"));
  (await page.clock.runFor(60000));
  assert.deepEqual(((await sent(page, "/api/save"))).length, 1);
});

test("test_panel_queues.APermanentRefusalIsNotRetried.test_a_permanent_refusal_still_guards_the_work", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  (await page.evaluate("window.__settle(0, 409)"));
  (await page.clock.runFor(30000));
  assert.ok((await page.evaluate("window.__unload()")), "the tab could close on work nothing will retry");
});

test("test_panel_queues.APermanentRefusalIsNotRetried.test_pressing_save_again_is_a_new_request_and_does_go", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__toggle(1)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 1"));
  (await page.evaluate("window.__settle(0, 409)"));
  (await page.clock.runFor(30000));
  (await page.evaluate("window.__toggle(2)"));
  (await page.evaluate("window.__save()"));
  (await page.waitForFunction("window.__sent('/api/save').length === 2"));
});

test("test_panel_queues.APermanentRefusalIsNotRetried.test_a_transient_refusal_is_still_retried", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__reorder(0, 3)"));
  (await page.clock.runFor(500));
  (await page.waitForFunction("window.__sent('/api/order').length === 1"));
  (await page.evaluate("window.__settle(0, 503)"));
  (await page.clock.runFor(2000));
  (await page.waitForFunction("window.__sent('/api/order').length === 2"));
});

test("test_panel_queues.TheHeartbeatHonoursTheSchedule.test_the_heartbeat_does_not_send_inside_a_pending_backoff", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("window.__reorder(0, 3)"));
  (await page.clock.runFor(500));
  (await page.waitForFunction("window.__sent('/api/order').length === 1"));
  for (const [i, wait] of [1200, 2200].map((value, index) => [index, value])) {
    (await page.evaluate(`window.__settle(${i}, 503)`));
    (await page.clock.runFor(wait));
    (await page.waitForFunction(`window.__sent('/api/order').length === ${(i + 2)}`));
  }
  (await page.evaluate("window.__settle(2, 503)"));
  (await page.waitForFunction("window.__sent('/api/order')[2].settled === true"));
  let before = ((await sent(page, "/api/order"))).length;
  (await page.clock.runFor(3500));
  assert.deepEqual(((await sent(page, "/api/order"))).length, before, "something sent inside the backoff window");
  (await page.clock.runFor(1000));
  (await page.waitForFunction(`window.__sent('/api/order').length === ${(before + 1)}`));
});

test("test_panel_queues.AHangingRequestDoesNotWedgeTheQueue.test_a_never_resolving_order_post_is_bounded", async ({ panel }) => {
  let page = (await open(panel, synth(8), {clock: true}));
  (await page.evaluate("window.__reorder(0, 3)"));
  (await page.clock.runFor(500));
  (await page.waitForFunction("window.__sent('/api/order').length === 1"));
  (await page.evaluate("window.__reorder(1, 5)"));
  (await page.clock.runFor(500));
  (await page.clock.runFor(30000));
  (await page.waitForFunction("window.__sent('/api/order').length >= 2", null, {timeout: 15000}));
});

test("test_panel_queues.AHangingRequestDoesNotWedgeTheQueue.test_the_newest_revision_is_the_one_that_goes", async ({ panel }) => {
  let page = (await open(panel, synth(8), {clock: true}));
  (await page.evaluate("window.__reorder(0, 3)"));
  (await page.clock.runFor(500));
  (await page.waitForFunction("window.__sent('/api/order').length === 1"));
  (await page.evaluate("window.__reorder(1, 6)"));
  (await page.clock.runFor(500));
  (await page.clock.runFor(30000));
  (await page.waitForFunction("window.__sent('/api/order').length >= 2", null, {timeout: 15000}));
  let calls = (await sent(page, "/api/order"));
  let live = (await page.evaluate("ITEMS.filter(x=>!x.isLogo).map(x=>x.key)"));
  assert.deepEqual(calls.at(-1)["body"]["order"], live, "the retry sent a stale arrangement");
});

test("test_panel_queues.CompleteSaveSnapshots.test_pack_only_edits_are_dirty_and_the_old_ack_does_not_clear_them", async ({ panel }) => {
  let page = (await open(panel, synth(6, {packs: [1, 1, 1, 2, 2, 2]}), {clock: true}));
  (await page.evaluate("__save(); ITEMS[1].pack=2; markSelDirty(); __settle(0,200)"));
  (await page.clock.runFor(20));
  assert.ok((await page.evaluate("selDirty() && __unload() && __dirtyShown()")));
  (await page.evaluate("__save(); __settle(1,200)"));
  (await page.clock.runFor(20));
  assert.ok(!(await page.evaluate("selDirty() || __unload()")));
});

test("test_panel_queues.CompleteSaveSnapshots.test_retry_does_not_submit_new_pack_intent_without_another_save", async ({ panel }) => {
  let page = (await open(panel, synth(6, {packs: [1, 1, 1, 2, 2, 2]}), {clock: true}));
  (await page.evaluate("__save(); __settle(0,503)"));
  (await page.clock.runFor(20));
  (await page.evaluate("ITEMS[1].pack=2; markSelDirty()"));
  (await page.clock.runFor(1100));
  let calls = (await sent(page, "/api/save"));
  assert.deepEqual((calls).length, 2);
  assert.deepEqual(calls[0]["body"], calls[1]["body"]);
  (await page.evaluate("__settle(1,200)"));
  (await page.clock.runFor(20));
  assert.ok((await page.evaluate("selDirty() && __unload()")));
});

test("test_panel_queues.CompleteSaveSnapshots.test_new_pack_only_body_outlives_old_permanent_refusal", async ({ panel }) => {
  for (const status of [400, 409]) {
    await test.step(`status=${status}`, async () => {
      let page = (await open(panel, synth(6, {packs: [1, 1, 1, 2, 2, 2]}), {clock: true}));
      (await page.evaluate(`__save(); ITEMS[1].pack=2; __save(); __settle(0,${status})`));
      (await page.clock.runFor(20));
      assert.deepEqual(((await sent(page, "/api/save"))).length, 2);
      (await page.evaluate("__settle(1,503)"));
      (await page.clock.runFor(1100));
      let calls = (await sent(page, "/api/save"));
      assert.deepEqual(calls[1]["body"], calls[2]["body"]);
      (await page.evaluate("__settle(2,200)"));
      (await page.clock.runFor(20));
      assert.ok(!(await page.evaluate("selDirty() || __unload()")));
    });
  }
});

test("test_panel_queues.CompleteSaveSnapshots.test_reset_restores_the_absence_of_a_pack_assignment", async ({ panel }) => {
  let page = (await open(panel, synth(6), {clock: true}));
  (await page.evaluate("remember(); ITEMS[1].pack=2; markSelDirty(); document.getElementById('resetAll').click()"));
  assert.ok(!(await page.evaluate("Object.hasOwn(ITEMS[1], 'pack')")));
  assert.ok(!(await page.evaluate("selDirty()")));
  (await page.evaluate("undo()"));
  assert.deepEqual((await page.evaluate("ITEMS[1].pack")), 2);
  (await page.evaluate("redo()"));
  assert.ok(!(await page.evaluate("Object.hasOwn(ITEMS[1], 'pack')")));
});

test("test_panel_queues.AcknowledgementsMustBeComplete.test_invalid_success_bodies_keep_both_queues_pending_and_retryable", async ({ panel }) => {
  for (const kind of ["order", "save"]) {
    for (const body of ["null", "[]", "{}", "{ok:false}", "{ok:true,count:-1}"]) {
      await test.step(`queue=${kind}, body=${body}`, async () => {
        let page = (await open(panel, synth(6), {clock: true}));
        (await page.evaluate(((kind === "order") ? "__reorder(0,3)" : "__toggle(1);__save()")));
        (await page.clock.runFor(450));
        (await page.evaluate(`__net.calls[0].res({ok:true,status:200,json:()=>Promise.resolve(${body})})`));
        (await page.clock.runFor(20));
        let [pending, flight] = ((kind === "order") ? ["pendingOrder", "orderFlight"] : ["pendingSel", "selFlight"]);
        assert.ok((await page.evaluate(`${pending}!==null && ${flight}===0 && __unload()`)));
        (await page.clock.runFor(1100));
        assert.deepEqual(((await sent(page, `/api/${kind}`))).length, 2);
        (await page.evaluate("__settle(1,200)"));
        (await page.clock.runFor(20));
        assert.equal((await page.evaluate(pending)) ?? null, null);
        assert.deepEqual((await page.evaluate("__rejections")), []);
      });
    }
  }
});

test("test_panel_queues.AcknowledgementsMustBeComplete.test_hanging_success_body_and_json_parse_error_do_not_acknowledge", async ({ panel }) => {
  for (const kind of ["order", "save"]) {
    for (const outcome of ["new Promise(()=>{})", "Promise.reject(new SyntaxError('truncated'))"]) {
      await test.step(`queue=${kind}, outcome=${outcome}`, async () => {
        let page = (await open(panel, synth(6), {clock: true}));
        (await page.evaluate(((kind === "order") ? "__reorder(0,3)" : "__toggle(1);__save()")));
        (await page.clock.runFor(450));
        (await page.evaluate(`__net.calls[0].res({ok:true,status:200,json:()=>${outcome}})`));
        (await page.clock.runFor((outcome.startsWith("new") ? 15050 : 20)));
        let pending = ((kind === "order") ? "pendingOrder" : "pendingSel");
        assert.notEqual((await page.evaluate(pending)) ?? null, null);
        assert.ok((await page.evaluate("__unload()")));
        assert.deepEqual((await page.evaluate("__rejections")), []);
      });
    }
  }
});

test("test_panel_queues.AcknowledgementsMustBeComplete.test_malformed_error_body_does_not_wedge_a_flight_or_loop_permanently", async ({ panel }) => {
  for (const kind of ["order", "save"]) {
    let page = (await open(panel, synth(6), {clock: true}));
    (await page.evaluate(((kind === "order") ? "__reorder(0,3)" : "__toggle(1);__save()")));
    (await page.clock.runFor(450));
    (await page.evaluate("__net.calls[0].res({ok:false,status:400,json:()=>Promise.resolve(null)})"));
    (await page.clock.runFor(60000));
    assert.deepEqual(((await sent(page, `/api/${kind}`))).length, 1);
    let flight = ((kind === "order") ? "orderFlight" : "selFlight");
    assert.deepEqual((await page.evaluate(flight)), 0);
    assert.ok((await page.evaluate("__unload()")));
    assert.deepEqual((await page.evaluate("__rejections")), []);
  }
});
