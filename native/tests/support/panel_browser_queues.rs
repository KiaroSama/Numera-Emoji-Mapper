//! Original queue revisions, dirty-state protection, refusals and request deadlines on a fake clock.
use super::{
    driver::{Browser, Case, Page},
    fixture::Server,
    layout::{model, synth},
};
use pyo3::prelude::*;
use serde_json::{Value, json};

fn sent(page: &Page<'_>, kind: &str) -> PyResult<Value> {
    page.value(&format!("__sent('/api/{kind}')"))
}
fn queue_case(items: &Value) -> Case<'_> {
    Case {
        clock: true,
        ..model(items)
    }
}
fn edit(kind: &str) -> &'static str {
    if kind == "order" {
        "__reorder(0,3)"
    } else {
        "__toggle(1);__save()"
    }
}
fn pending(kind: &str) -> &'static str {
    if kind == "order" {
        "pendingOrder"
    } else {
        "pendingSel"
    }
}

pub fn run(browser: &Browser<'_>, server: &Server) -> PyResult<()> {
    let url = &server.url;
    let six = synth(6, false, "static", None, &[]);
    browser.case("old-permanent-order-refusal", url, true, |p| {
        p.eval("__reorder(0,3)")?;
        p.tick(450)?;
        p.eval("__reorder(0,2)")?;
        let newer = p.value("pendingOrder")?;
        p.tick(450)?;
        p.eval("__settle(0,400)")?;
        p.tick(1000)?;
        let calls = sent(p, "order")?;
        assert_eq!(calls.as_array().unwrap().len(), 2);
        assert_eq!(calls[1]["body"]["order"], newer);
        p.eval("__settle(1,200)")?;
        p.tick(50)?;
        assert_eq!(p.value("pendingOrder")?, Value::Null);
        Ok(())
    })?;
    for kind in ["order", "save"] {
        for status in [400, 409] {
            browser.case(&format!("new-body-{kind}-{status}"), url, true, |p| {
                p.eval(if kind == "order" {
                    "__reorder(0,3)"
                } else {
                    "__toggle(0);__save()"
                })?;
                p.tick(450)?;
                p.eval(if kind == "order" {
                    "__reorder(0,2)"
                } else {
                    "__toggle(1);__save()"
                })?;
                p.tick(450)?;
                p.eval(&format!("__settle(0,{status})"))?;
                p.tick(500)?;
                assert_eq!(sent(p, kind)?.as_array().unwrap().len(), 2);
                p.eval("__settle(1,503)")?;
                p.tick(1100)?;
                let calls = sent(p, kind)?;
                assert_eq!(calls.as_array().unwrap().len(), 3);
                assert_eq!(calls[1]["body"], calls[2]["body"]);
                p.eval("__settle(2,200)")?;
                p.tick(50)?;
                assert_eq!(p.value(pending(kind))?, Value::Null);
                Ok(())
            })?;
        }
    }
    browser.case("unsaved-edit-protected", url, model(&six), |p| {
        assert_eq!(p.value("__dirty()")?, json!(false));
        p.eval("__toggle(1)")?;
        for expression in ["__dirty()", "__dirtyShown()", "__unload()"] {
            assert_eq!(p.value(expression)?, json!(true));
        }
        Ok(())
    })?;
    browser.case("edit-during-save-survives", url, model(&six), |p| {
        p.eval("__toggle(1);__save()")?;
        p.wait("__sent('/api/save').length===1")?;
        assert_eq!(
            sent(p, "save")?[0]["body"]["excluded"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        p.eval("__toggle(2);__settle(0,200)")?;
        p.wait("__sent('/api/save')[0].settled===true")?;
        assert_eq!(p.value("__excluded().length")?, json!(2));
        assert_eq!(p.value("__dirty()")?, json!(true));
        assert_eq!(p.value("__unload()")?, json!(true));
        Ok(())
    })?;
    browser.case("newer-save-cleans-selection", url, model(&six), |p| {
        p.eval("__toggle(1);__save()")?;
        p.wait("__sent('/api/save').length===1")?;
        p.eval("__toggle(2);__settle(0,200)")?;
        p.eval("__save()")?;
        p.wait("__sent('/api/save').length===2")?;
        p.eval("__settle(1,200)")?;
        p.wait("__dirty()===false")?;
        assert_eq!(p.value("__unload()")?, json!(false));
        Ok(())
    })?;
    browser.case("undo-to-acknowledged-clean", url, model(&six), |p| {
        p.eval("__toggle(1)")?;
        assert_eq!(p.value("__dirty()")?, json!(true));
        p.eval("__toggle(1)")?;
        assert_eq!(p.value("__dirty()")?, json!(false));
        assert_eq!(p.value("__unload()")?, json!(false));
        Ok(())
    })?;
    browser.case(
        "transient-selection-dirty-retry",
        url,
        queue_case(&six),
        |p| {
            p.eval("__toggle(1);__save()")?;
            p.wait("__sent('/api/save').length===1")?;
            p.eval("__settle(0,503)")?;
            p.wait("__sent('/api/save')[0].settled===true")?;
            assert_eq!(p.value("__dirty()")?, json!(true));
            p.tick(2000)?;
            p.wait("__sent('/api/save').length===2")?;
            p.eval("__settle(1,200)")?;
            p.wait("__dirty()===false")
        },
    )?;
    server.reset()?;
    browser.case("selection-reaches-catalog", url, false, |p| {
        p.eval("__net.passthrough=true;__toggle(1)")?;
        let key = p.value("ITEMS[1].key")?;
        p.eval("__save()")?;
        p.wait("__dirty()===false")?;
        assert!(server.excluded()?.iter().any(|value| json!(value) == key));
        Ok(())
    })?;
    server.reset()?;
    for (kind, status) in [("order", 400), ("save", 409)] {
        browser.case(
            &format!("permanent-{kind}-no-retry"),
            url,
            queue_case(&six),
            |p| {
                p.eval(edit(kind))?;
                p.tick(500)?;
                p.wait(&format!("__sent('/api/{kind}').length===1"))?;
                p.eval(&format!("__settle(0,{status})"))?;
                p.tick(60_000)?;
                assert_eq!(sent(p, kind)?.as_array().unwrap().len(), 1);
                Ok(())
            },
        )?;
    }
    browser.case(
        "permanent-refusal-protects-work",
        url,
        queue_case(&six),
        |p| {
            p.eval("__toggle(1);__save()")?;
            p.wait("__sent('/api/save').length===1")?;
            p.eval("__settle(0,409)")?;
            p.tick(30_000)?;
            assert_eq!(p.value("__unload()")?, json!(true));
            Ok(())
        },
    )?;
    browser.case("explicit-save-after-refusal", url, queue_case(&six), |p| {
        p.eval("__toggle(1);__save()")?;
        p.wait("__sent('/api/save').length===1")?;
        p.eval("__settle(0,409)")?;
        p.tick(30_000)?;
        p.eval("__toggle(2);__save()")?;
        p.wait("__sent('/api/save').length===2")
    })?;
    browser.case(
        "transient-order-still-retries",
        url,
        queue_case(&six),
        |p| {
            p.eval("__reorder(0,3)")?;
            p.tick(500)?;
            p.wait("__sent('/api/order').length===1")?;
            p.eval("__settle(0,503)")?;
            p.tick(2000)?;
            p.wait("__sent('/api/order').length===2")
        },
    )?;
    browser.case("heartbeat-obeys-backoff", url, queue_case(&six), |p| {
        p.eval("__reorder(0,3)")?;
        p.tick(500)?;
        p.wait("__sent('/api/order').length===1")?;
        for (i, wait) in [1200, 2200].into_iter().enumerate() {
            p.eval(&format!("__settle({i},503)"))?;
            p.tick(wait)?;
            p.wait(&format!("__sent('/api/order').length==={}", i + 2))?;
        }
        p.eval("__settle(2,503)")?;
        p.wait("__sent('/api/order')[2].settled===true")?;
        let before = sent(p, "order")?.as_array().unwrap().len();
        p.tick(3500)?;
        assert_eq!(sent(p, "order")?.as_array().unwrap().len(), before);
        p.tick(1000)?;
        p.wait(&format!("__sent('/api/order').length==={}", before + 1))
    })?;
    let eight = synth(8, false, "static", None, &[]);
    for target in [5, 6] {
        browser.case(
            &format!("hanging-request-{target}"),
            url,
            queue_case(&eight),
            |p| {
                p.eval("__reorder(0,3)")?;
                p.tick(500)?;
                p.wait("__sent('/api/order').length===1")?;
                p.eval(&format!("__reorder(1,{target})"))?;
                p.tick(500)?;
                p.tick(30_000)?;
                p.wait("__sent('/api/order').length>=2")?;
                if target == 6 {
                    let calls = sent(p, "order")?;
                    assert_eq!(
                        calls.as_array().unwrap().last().unwrap()["body"]["order"],
                        p.value("ITEMS.filter(x=>!x.isLogo).map(x=>x.key)")?
                    );
                }
                Ok(())
            },
        )?;
    }
    let packed = synth(6, false, "static", Some(&[1, 1, 1, 2, 2, 2]), &[]);
    browser.case("pack-edits-survive-ack", url, queue_case(&packed), |p| {
        p.eval("__save();ITEMS[1].pack=2;markSelDirty();__settle(0,200)")?;
        p.tick(20)?;
        assert_eq!(
            p.value("selDirty()&&__unload()&&__dirtyShown()")?,
            json!(true)
        );
        p.eval("__save();__settle(1,200)")?;
        p.tick(20)?;
        assert_eq!(p.value("selDirty()||__unload()")?, json!(false));
        Ok(())
    })?;
    browser.case(
        "pack-retry-immutable-snapshot",
        url,
        queue_case(&packed),
        |p| {
            p.eval("__save();__settle(0,503)")?;
            p.tick(20)?;
            p.eval("ITEMS[1].pack=2;markSelDirty()")?;
            p.tick(1100)?;
            let calls = sent(p, "save")?;
            assert_eq!(calls.as_array().unwrap().len(), 2);
            assert_eq!(calls[0]["body"], calls[1]["body"]);
            p.eval("__settle(1,200)")?;
            p.tick(20)?;
            assert_eq!(p.value("selDirty()&&__unload()")?, json!(true));
            Ok(())
        },
    )?;
    for status in [400, 409] {
        browser.case(
            &format!("pack-body-old-refusal-{status}"),
            url,
            queue_case(&packed),
            |p| {
                p.eval(&format!(
                    "__save();ITEMS[1].pack=2;__save();__settle(0,{status})"
                ))?;
                p.tick(20)?;
                assert_eq!(sent(p, "save")?.as_array().unwrap().len(), 2);
                p.eval("__settle(1,503)")?;
                p.tick(1100)?;
                let calls = sent(p, "save")?;
                assert_eq!(calls[1]["body"], calls[2]["body"]);
                p.eval("__settle(2,200)")?;
                p.tick(20)?;
                assert_eq!(p.value("selDirty()||__unload()")?, json!(false));
                Ok(())
            },
        )?;
    }
    browser.case("reset-restores-absent-pack", url, queue_case(&six), |p| {
        p.eval(
            "remember();ITEMS[1].pack=2;markSelDirty();document.getElementById('resetAll').click()",
        )?;
        assert_eq!(p.value("Object.hasOwn(ITEMS[1],'pack')")?, json!(false));
        assert_eq!(p.value("selDirty()")?, json!(false));
        p.eval("undo()")?;
        assert_eq!(p.value("ITEMS[1].pack")?, json!(2));
        p.eval("redo()")?;
        assert_eq!(p.value("Object.hasOwn(ITEMS[1],'pack')")?, json!(false));
        Ok(())
    })?;
    for kind in ["order", "save"] {
        for (i, body) in ["null", "[]", "{}", "{ok:false}", "{ok:true,count:-1}"]
            .into_iter()
            .enumerate()
        {
            browser.case(&format!("invalid-ack-{kind}-{i}"),url,queue_case(&six),|p| {
                p.eval(edit(kind))?;p.tick(450)?;
                p.eval(&format!("__net.calls[0].res({{ok:true,status:200,json:()=>Promise.resolve({body})}})"))?;p.tick(20)?;
                let flight=if kind=="order" {"orderFlight"} else {"selFlight"};
                assert_eq!(p.value(&format!("{}!==null&&{flight}===0&&__unload()",pending(kind)))?,json!(true));
                p.tick(1100)?;assert_eq!(sent(p,kind)?.as_array().unwrap().len(),2);
                p.eval("__settle(1,200)")?;p.tick(20)?;assert_eq!(p.value(pending(kind))?,Value::Null);
                assert_eq!(p.value("__rejections")?,json!([]));Ok(())
            })?;
        }
        for (i, outcome) in [
            "new Promise(()=>{})",
            "Promise.reject(new SyntaxError('truncated'))",
        ]
        .into_iter()
        .enumerate()
        {
            browser.case(
                &format!("broken-ack-body-{kind}-{i}"),
                url,
                queue_case(&six),
                |p| {
                    p.eval(edit(kind))?;
                    p.tick(450)?;
                    p.eval(&format!(
                        "__net.calls[0].res({{ok:true,status:200,json:()=>{outcome}}})"
                    ))?;
                    p.tick(if i == 0 { 15050 } else { 20 })?;
                    assert_ne!(p.value(pending(kind))?, Value::Null);
                    assert_eq!(p.value("__unload()")?, json!(true));
                    assert_eq!(p.value("__rejections")?, json!([]));
                    Ok(())
                },
            )?;
        }
        browser.case(
            &format!("malformed-error-{kind}"),
            url,
            queue_case(&six),
            |p| {
                p.eval(edit(kind))?;
                p.tick(450)?;
                p.eval("__net.calls[0].res({ok:false,status:400,json:()=>Promise.resolve(null)})")?;
                p.tick(60_000)?;
                assert_eq!(sent(p, kind)?.as_array().unwrap().len(), 1);
                assert_eq!(
                    p.value(if kind == "order" {
                        "orderFlight"
                    } else {
                        "selFlight"
                    })?,
                    json!(0)
                );
                assert_eq!(p.value("__unload()")?, json!(true));
                assert_eq!(p.value("__rejections")?, json!([]));
                Ok(())
            },
        )?;
    }
    Ok(())
}
