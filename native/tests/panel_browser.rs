//! Real DOM/clock/catalog tests assigned to the browser job, never source-text substitutes.
#[path = "support/panel_browser_curation.rs"]
mod curation;
#[path = "support/panel_browser_driver.rs"]
mod driver;
#[path = "support/panel_browser_fixture.rs"]
mod fixture;
#[path = "support/panel_browser_layout.rs"]
mod layout;
#[path = "support/panel_browser_media.rs"]
mod media;
#[path = "support/panel_browser_perf.rs"]
mod perf;
#[path = "support/panel_browser_queues.rs"]
mod queues;
use pyo3::prelude::*;
use serde_json::{Value, json};

fn failed_selection(page: &driver::Page<'_>) -> PyResult<Value> {
    page.click_card(1)?;
    page.click("#save")?;
    let saves = page.value("__sent('/api/save')")?;
    assert_eq!(
        saves.as_array().unwrap().len(),
        1,
        "Save did not reach the panel"
    );
    page.eval(&format!("__settle({}, 500)", saves[0]["i"]))?;
    page.tick(50)?;
    assert_eq!(
        page.value("document.getElementById('alert').classList.contains('show')")?,
        json!(true)
    );
    Ok(saves[0]["body"]["excluded"].clone())
}

#[test]
fn original_revisioned_order_and_failed_selection_workflows_retain_real_browser_assertions() {
    if std::env::var("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS").as_deref() == Ok("1") {
        eprintln!("Browser acceptance explicitly omitted: NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS=1");
        return;
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let mut server = fixture::Server::start(py)?;
        let mut browser = driver::Browser::start(py, &server.root)?;
        let result = (|| -> PyResult<()> {
            browser.case("old-order-ack", &server.url, true, |page| {
                page.eval("__reorder(0, 5)")?;
                page.tick(500)?;
                assert_eq!(page.value("__sent('/api/order').length")?, json!(1));
                page.eval("__reorder(1, 7)")?;
                let newer = page.value("pendingOrder")?;
                page.eval("__settle(0, 200)")?;
                page.tick(50)?;
                assert_eq!(page.value("pendingOrder")?, newer);
                assert_eq!(page.value("__unload()")?, json!(true));
                page.tick(2000)?;
                let sent = page.value("__sent('/api/order')")?;
                assert_eq!(sent.as_array().unwrap().len(), 2);
                assert_eq!(sent[1]["body"]["order"], newer);
                page.eval("__settle(1, 200)")?;
                page.tick(50)?;
                assert_eq!(page.value("pendingOrder")?, Value::Null);
                assert_eq!(page.value("__unload()")?, json!(false));
                Ok(())
            })?;
            browser.case("failed-old-order", &server.url, true, |page| {
                page.eval("__reorder(0, 5)")?;
                page.tick(500)?;
                let stale = page.value("__sent('/api/order')[0].body.order")?;
                page.eval("__reorder(1, 7)")?;
                let newer = page.value("pendingOrder")?;
                assert_ne!(stale, newer);
                page.eval("__settle(0, 500)")?;
                page.tick(50)?;
                assert_eq!(page.value("pendingOrder")?, newer);
                page.tick(3000)?;
                assert_eq!(page.value("__sent('/api/order').at(-1).body.order")?, newer);
                Ok(())
            })?;
            browser.case("single-order-flight", &server.url, true, |page| {
                page.eval("__reorder(0, 5)")?;
                page.tick(450)?;
                page.eval("(()=>{ flushOrder(pendingOrder); flushOrder(pendingOrder); })()")?;
                assert_eq!(page.value("__sent('/api/order').length")?, json!(1));
                page.eval("__reorder(1, 7)")?;
                page.eval("(()=>{ flushOrder(pendingOrder); })()")?;
                page.tick(1000)?;
                assert_eq!(page.value("__sent('/api/order').length")?, json!(1));
                page.eval("__settle(0, 200)")?;
                page.tick(50)?;
                assert_eq!(page.value("__sent('/api/order').length")?, json!(2));
                assert_eq!(
                    page.value("__sent('/api/order')[1].body.order")?,
                    page.value("pendingOrder")?
                );
                Ok(())
            })?;
            browser.case(
                "failed-selection-keeps-warning",
                &server.url,
                true,
                |page| {
                    failed_selection(page)?;
                    assert_eq!(page.value("__unload()")?, json!(true));
                    page.tick(6000)?;
                    assert_eq!(
                        page.value("document.getElementById('alert').classList.contains('show')")?,
                        json!(true)
                    );
                    assert_eq!(page.value("__unload()")?, json!(true));
                    let pending = page.value("__sent('/api/save').filter(c=>!c.settled)")?;
                    assert_eq!(pending.as_array().unwrap().len(), 1);
                    page.eval(&format!("__settle({}, 200)", pending[0]["i"]))?;
                    page.tick(50)?;
                    assert_eq!(
                        page.value("document.getElementById('alert').classList.contains('show')")?,
                        json!(false)
                    );
                    assert_eq!(page.value("__unload()")?, json!(false));
                    Ok(())
                },
            )?;
            browser.case("failed-order-reaches-catalog", &server.url, true, |page| {
                page.eval("__reorder(0, 5)")?;
                page.tick(500)?;
                let stale = page.value("__sent('/api/order')[0].body.order")?;
                page.eval("__reorder(1, 7)")?;
                let newer = page.value("pendingOrder")?;
                assert_ne!(stale, newer);
                page.eval("__net.passthrough = true")?;
                page.eval("__settle(0, 500)")?;
                page.tick(3000)?;
                page.wait_saved()?;
                let expected: Vec<String> = serde_json::from_value(newer.clone()).unwrap();
                assert_eq!(server.order()?, expected);
                let posted = page.posted_orders()?;
                assert!(!posted.is_empty(), "retry never reached the real server");
                assert!(
                    posted.iter().all(|body| body["order"] != stale),
                    "stale arrangement was transmitted"
                );
                page.reload()?;
                assert_eq!(
                    page.value("ITEMS.filter(x=>!x.isLogo).map(x=>x.key)")?,
                    newer
                );
                Ok(())
            })?;
            browser.case(
                "selection-retry-keeps-save-snapshot",
                &server.url,
                true,
                |page| {
                    let asked = failed_selection(page)?;
                    page.click_card(2)?;
                    page.tick(6000)?;
                    let saves = page.value("__sent('/api/save')")?;
                    let saves = saves.as_array().unwrap();
                    assert!(saves.len() > 1, "refused selection was not retried");
                    assert_eq!(saves.last().unwrap()["body"]["excluded"], asked);
                    Ok(())
                },
            )?;
            browser.case(
                "order-ack-keeps-selection-warning",
                &server.url,
                true,
                |page| {
                    failed_selection(page)?;
                    page.eval("__reorder(0, 2)")?;
                    page.tick(500)?;
                    let order = page.value("__sent('/api/order').at(-1)")?;
                    page.eval(&format!("__settle({}, 200)", order["i"]))?;
                    page.tick(50)?;
                    assert_eq!(
                        page.value("document.getElementById('alert').classList.contains('show')")?,
                        json!(true)
                    );
                    Ok(())
                },
            )?;
            browser.case("actual-drag-persistence", &server.url, false, |page| {
                page.eval("__net.passthrough = true")?;
                let before = server.order()?;
                page.drag(0, 5)?;
                let moved = page.value("ITEMS.map(x=>x.key)")?;
                assert_ne!(moved, json!(before));
                let expected: Vec<String> = serde_json::from_value(moved).unwrap();
                page.wait_saved()?;
                assert_eq!(
                    server.order()?,
                    expected,
                    "acknowledged order was not committed"
                );
                page.reload()?;
                assert_eq!(
                    page.value("ITEMS.filter(x=>!x.isLogo).map(x=>x.key)")?,
                    json!(expected)
                );
                Ok(())
            })?;
            layout::run(&browser, &server.url)?;
            queues::run(&browser, &server)?;
            curation::run(&browser, &server)?;
            perf::run(&browser, &server.url)?;
            let correspondence: Value = serde_json::from_str(include_str!("../../tests/fixtures/native/browser-cases.json")).unwrap();
            let expected = correspondence["sources"].as_array().unwrap().iter()
                .flat_map(|source| source["cases"].as_array().unwrap())
                .flat_map(|case| case[1].as_array().unwrap())
                .map(|scenario| scenario.as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(browser.completed(), expected,
                "browser correspondence requires every actual scenario, not just source declarations");
            Ok(())
        })();
        let browser_closed = browser.close();
        let server_closed = server.close();
        for cleanup in [&browser_closed, &server_closed] {
            if let Err(error) = cleanup { eprintln!("ERROR: browser cohort cleanup failed: {error}"); }
        }
        result?;
        browser_closed?;
        server_closed?;
        server.clean()?;
        Ok(())
    })
    .unwrap();
}
