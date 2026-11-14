//! Original curation gestures, capacity, history, windowed tray and draft recovery.
use super::{
    driver::{Browser, Case, Page},
    fixture::Server,
    layout::{model, synth},
};
use pyo3::prelude::*;
use serde_json::{Value, json};

fn count(p: &Page<'_>, selector: &str) -> PyResult<u64> {
    p.locator(selector)?.call_method0("count")?.extract()
}
fn text(p: &Page<'_>, selector: &str) -> PyResult<String> {
    p.locator(selector)?.call_method0("inner_text")?.extract()
}
fn hold(p: &Page<'_>, n: usize) -> PyResult<()> {
    p.click("#selmode")?;
    p.eval(&format!(
        "holdKeys(new Set(ITEMS.filter(x=>!x.isLogo&&x.included).slice(0,{n}).map(x=>x.key)))"
    ))?;
    p.wait(&format!(
        "document.querySelectorAll('#holdCards .hcard').length==={n}"
    ))
}
fn picked(p: &Page<'_>, n: usize) -> PyResult<()> {
    p.wait(&format!("picked.size==={n}"))
}
fn ticks(p: &Page<'_>) -> PyResult<Value> {
    p.value("[...document.querySelectorAll('#holdCards .hcard')].map(c=>getComputedStyle(c.querySelector('.pick'),'::after').content)")
}
fn tray_end(p: &Page<'_>) -> PyResult<String> {
    p.eval("const h=document.getElementById('holding');h.scrollLeft=h.scrollWidth")?;
    let last = p.value("heldOrder()[599]")?.as_str().unwrap().to_owned();
    p.wait(&format!(
        "!!document.querySelector('#holdCards .hcard[data-key=\"{last}\"]')"
    ))?;
    Ok(last)
}
fn ring(p: &Page<'_>, selector: &str) -> PyResult<Value> {
    p.value(&format!("(()=>{{const n=document.querySelector({});return n?{{anim:getComputedStyle(n,'::before').animationName,play:n.getAnimations({{subtree:true}}).map(a=>a.playState)}}:null;}})()",json!(selector)))
}
fn hold_first(p: &Page<'_>) -> PyResult<()> {
    p.click("#selmode")?;
    p.click_at(".card .pick", 0, false)?;
    p.click("#toHold")?;
    assert_eq!(count(p, "#holdCards .hcard")?, 1);
    Ok(())
}

pub fn run(browser: &Browser<'_>, server: &Server) -> PyResult<()> {
    let url = &server.url;
    let twelve = synth(12, false, "static", None, &[]);
    let packed = synth(
        12,
        false,
        "static",
        Some(&[1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2]),
        &[],
    );
    browser.case("toolbar-bounds",url,model(&twelve),|p| {
        for width in [1920,1200] {
            p.viewport(width,900)?;
            let brand:Value=p.value("(()=>{const r=document.querySelector('header h1').getBoundingClientRect();return {x:r.x,width:r.width}})()")?;
            let toolbar=p.value("(()=>{const r=document.querySelector('header .actions').getBoundingClientRect();return {x:r.x,width:r.width}})()")?;
            let save=p.value("(()=>{const r=document.querySelector('header .hright').getBoundingClientRect();return {x:r.x,width:r.width}})()")?;
            assert!(brand["x"].as_f64().unwrap()+brand["width"].as_f64().unwrap()<=toolbar["x"].as_f64().unwrap());
            assert!(toolbar["x"].as_f64().unwrap()+toolbar["width"].as_f64().unwrap()<=save["x"].as_f64().unwrap());
        }Ok(())
    })?;
    browser.case("hold-unhold-history", url, model(&packed), |p| {
        p.click("#selmode")?;
        assert_eq!(count(p, ".tick:visible")?, 0);
        p.click_at(".card .pick", 1, false)?;
        p.click_at(".card .pick", 4, true)?;
        assert_eq!(count(p, ".card.picked")?, 4);
        p.click("#toHold")?;
        assert_eq!(count(p, "#holdCards .hcard")?, 4);
        assert_eq!(count(p, "#grid .card")?, 8);
        assert_eq!(
            p.locator(".packsep .n")?
                .call_method1("nth", (0,))?
                .call_method0("inner_text")?
                .extract::<String>()?,
            "#1–#2"
        );
        p.click("#undo")?;
        assert_eq!(count(p, "#grid .card")?, 12);
        assert_eq!(count(p, ".card.picked")?, 4);
        p.click("#redo")?;
        assert_eq!(count(p, "#holdCards .hcard")?, 4);
        p.click("#unholdAll")?;
        assert_eq!(count(p, "#grid .card")?, 12);
        assert_eq!(count(p, "#holdCards .hcard")?, 0);
        p.click("#undo")?;
        assert_eq!(count(p, "#holdCards .hcard")?, 4);
        p.click_at("#holdCards .hcard button", 0, false)?;
        assert_eq!(count(p, "#holdCards .hcard")?, 3);
        Ok(())
    })?;
    let same_pack = synth(12, false, "static", Some(&[1; 12]), &[]);
    browser.case("held-multi-selection", url, model(&same_pack), |p| {
        p.click("#selmode")?;
        p.click_at(".card .pick", 0, false)?;
        p.click_at(".card .pick", 4, true)?;
        p.click("#toHold")?;
        assert_eq!(count(p, "#holdCards .hcard")?, 5);
        assert_eq!(count(p, "#holdCards .hcard .pick")?, 5);
        p.click_at("#holdCards .hcard .pick", 0, false)?;
        p.click_at("#holdCards .hcard .pick", 3, true)?;
        assert_eq!(count(p, "#holdCards .hcard.picked")?, 4);
        assert_eq!(text(p, "#selLabel")?, "Selection: 4 picked");
        p.click_at("#holdCards .hcard.picked button", 0, false)?;
        assert_eq!(count(p, "#holdCards .hcard")?, 1);
        assert_eq!(count(p, "#grid .card")?, 11);
        Ok(())
    })?;
    browser.case("held-destination-stamp-history",url,model(&packed),|p| {
        hold_first(p)?;
        let moved=p.value("()=>{const it=ITEMS.find(x=>!x.included&&!x.isLogo);const from=it.pack;const other=packStarts().starts.find(s=>s.pack!==from);remember();holdDragKeys=new Set([it.key]);carried.clear();carried.add(it.key);aimAt(other.index);ITEMS.splice(ITEMS.indexOf(it),1);ITEMS.splice(other.index+1,0,it);const accepted=acceptDrop();return {accepted,from,into:other.pack,now:it.pack,included:it.included,assigned:assignedPacks().get(it.key),runs:packStarts().starts.map(s=>s.pack)};}")?;
        assert_eq!(moved["accepted"],true);assert_eq!(moved["included"],true);
        assert_eq!(moved["now"],moved["into"]);assert_eq!(moved["assigned"],moved["into"]);
        assert_eq!(moved["runs"],json!([moved["from"],moved["into"]]));assert_eq!(count(p,"#holdCards .hcard")?,0);
        p.click("#undo")?;assert_eq!(count(p,"#holdCards .hcard")?,1);
        assert_eq!(p.value("ITEMS.find(x=>!x.included&&!x.isLogo).pack")?,moved["from"]);Ok(())
    })?;
    let full_packs = (0..202)
        .map(|i| if i < 2 { 1 } else { 2 })
        .collect::<Vec<_>>();
    let full = synth(202, false, "static", Some(&full_packs), &[0]);
    browser.case("full-destination-refuses",url,model(&full),|p| {
        assert_eq!(count(p,"#holdCards .hcard")?,1);
        let refused=p.value("()=>{const it=ITEMS.find(x=>!x.included&&!x.isLogo);const full=packStarts().starts.find(s=>s.pack!==it.pack);holdDragKeys=new Set([it.key]);carried.clear();carried.add(it.key);aimAt(full.index);ITEMS.splice(ITEMS.indexOf(it),1);ITEMS.splice(full.index+1,0,it);let said=null;const real=toast;toast=m=>{said=m};const accepted=acceptDrop();toast=real;return {accepted,said,held:!it.included,into:full.pack,pack:it.pack};}")?;
        assert_eq!(refused["accepted"],false);assert_eq!(refused["held"],true);assert_eq!(refused["pack"],1);
        let said=refused["said"].as_str().unwrap();
        assert!(said.contains(&format!("Pack {} is full (200/200)",refused["into"])));assert!(said.contains("Hold one of its emoji first"));Ok(())
    })?;
    let overflow = (0..203)
        .map(|i| if i < 201 { 1 } else { 2 })
        .collect::<Vec<_>>();
    let overflow = synth(203, false, "static", Some(&overflow), &[195, 202]);
    browser.case("unhold-all-atomic-refusal", url, model(&overflow), |p| {
        p.click("#unholdAll")?;
        assert_eq!(count(p, "#holdCards .hcard")?, 2);
        assert!(text(p, "#toast")?.contains("Pack 1 is full (200/200)"));
        p.click_at("#holdCards .hcard button", 1, false)?;
        assert_eq!(count(p, "#holdCards .hcard")?, 1);
        Ok(())
    })?;
    let six = synth(6, false, "static", None, &[]);
    browser.case(
        "reset-submitted-save-history",
        url,
        Case {
            clock: true,
            ..model(&six)
        },
        |p| {
            for control in ["#anim", "#bg", "#selmode"] {
                p.click(control)?;
            }
            p.click_at(".card .pick", 0, false)?;
            p.click("#toHold")?;
            p.click("#save")?;
            p.wait("__sent('/api/save').length===1")?;
            let expected = p.value("snapshot()")?;
            p.click_at(".card .pick", 0, false)?;
            p.click("#toHold")?;
            p.click("#anim")?;
            p.eval("__settle(0,200)")?;
            p.tick(50)?;
            assert_eq!(p.value("selDirty()")?, json!(true));
            let edited = p.value("snapshot()")?;
            p.click("#resetAll")?;
            assert_eq!(p.value("snapshot()")?, expected);
            p.click("#undo")?;
            assert_eq!(p.value("snapshot()")?, edited);
            p.click("#redo")?;
            assert_eq!(p.value("snapshot()")?, expected);
            Ok(())
        },
    )?;
    server.reset()?;
    browser.case("hold-save-real-catalog", url, false, |p| {
        p.eval("__net.passthrough=true")?;
        p.click("#selmode")?;
        let key = p.value("document.querySelector('.card').dataset.key")?;
        p.click_at(".card .pick", 0, false)?;
        p.click("#toHold")?;
        p.click("#save")?;
        p.wait("pendingSel===null&&!selDirty()")?;
        assert_eq!(json!(server.excluded()?), json!([key]));
        p.reload()?;
        assert_eq!(count(p, "#holdCards .hcard")?, 1);
        assert_eq!(count(p, &format!("#grid .card[data-key={}]", key))?, 0);
        Ok(())
    })?;
    server.reset()?;
    let six_packed = synth(6, false, "static", Some(&[1; 6]), &[]);
    browser.case("native-drag-history-slot", url, model(&six_packed), |p| {
        p.click("#selmode")?;
        p.click_at(".card .pick", 1, false)?;
        p.click("#toHold")?;
        p.drag(0, 4)?;
        assert_eq!(
            p.value("document.querySelector('#grid .card').dataset.key")?,
            six_packed[2]["key"]
        );
        p.click("#undo")?;
        assert_eq!(
            p.value("document.querySelector('#grid .card').dataset.key")?,
            six_packed[0]["key"]
        );
        p.click("#redo")?;
        p.click("#holdCards .hcard button")?;
        assert_eq!(
            p.value("document.querySelectorAll('#grid .card')[1].dataset.key")?,
            six_packed[1]["key"]
        );
        Ok(())
    })?;
    browser.case("held-tick-empty-until-picked", url, model(&twelve), |p| {
        hold(p, 5)?;
        assert!(
            ticks(p)?
                .as_array()
                .unwrap()
                .iter()
                .all(|value| value == "none")
        );
        p.click_at("#holdCards .hcard img", 1, false)?;
        picked(p, 1)?;
        let drawn = ticks(p)?
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, v)| v.as_str() != Some("none"))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(drawn, vec![1]);
        Ok(())
    })?;
    browser.case("held-face-shift-range", url, model(&twelve), |p| {
        hold(p, 5)?;
        p.click_at("#holdCards .hcard img", 0, false)?;
        picked(p, 1)?;
        p.click_at("#holdCards .hcard img", 3, true)?;
        picked(p, 4)
    })?;
    browser.case("held-box-one-toggle", url, model(&twelve), |p| {
        hold(p, 5)?;
        p.click_at("#holdCards .hcard .pick", 0, false)?;
        picked(p, 1)
    })?;
    browser.case(
        "unhold-preserves-other-selection",
        url,
        model(&twelve),
        |p| {
            hold(p, 5)?;
            p.click_at("#holdCards .hcard img", 0, false)?;
            p.click_at("#holdCards .hcard img", 2, false)?;
            picked(p, 2)?;
            p.click_at("#holdCards .hcard button", 4, false)?;
            p.wait("document.querySelectorAll('#holdCards .hcard').length===4")?;
            picked(p, 2)
        },
    )?;
    browser.case("held-outside-selection", url, model(&twelve), |p| {
        hold(p, 5)?;
        p.click("#selmode")?;
        p.click_at("#holdCards .hcard img", 0, false)?;
        picked(p, 0)
    })?;
    browser.case("tray-drag-selected-group", url, model(&packed), |p| {
        hold(p, 3)?;
        p.click_at("#holdCards .hcard img", 0, false)?;
        p.click_at("#holdCards .hcard img", 2, true)?;
        assert_eq!(p.value("heldPicked().length")?, json!(3));
        p.tray_drag(0, None)?;
        assert_eq!(p.value("holdDragKeys.size")?, json!(3));
        Ok(())
    })?;
    browser.case("tray-drag-unselected-single", url, model(&packed), |p| {
        hold(p, 3)?;
        p.click_at("#holdCards .hcard img", 0, false)?;
        picked(p, 1)?;
        p.tray_drag(2, None)?;
        assert_eq!(p.value("holdDragKeys.size")?, json!(1));
        Ok(())
    })?;
    browser.case("tray-drop-removes-held-node", url, model(&packed), |p| {
        hold(p, 1)?;
        let key = p.value("document.querySelector('#holdCards .hcard').dataset.key")?;
        p.tray_drag(0, Some(3))?;
        assert_eq!(
            p.value(&format!("ITEMS.find(x=>x.key==={key}).included"))?,
            json!(true)
        );
        assert_eq!(count(p, &format!("#holdCards .hcard[data-key={key}]"))?, 0);
        assert_eq!(text(p, "#holdCount")?, "0");
        Ok(())
    })?;
    let large = synth(600, false, "static", None, &[]);
    browser.case("large-tray-window", url, model(&large), |p| {
        p.click("#none")?;
        assert_eq!(p.value("heldOrder().length")?, json!(600));
        assert!(count(p, "#holdCards .hcard")? <= 60);
        tray_end(p)?;
        assert!(count(p, "#holdCards .hcard")? <= 60);
        Ok(())
    })?;
    browser.case(
        "large-tray-shift-unmounted-range",
        url,
        model(&large),
        |p| {
            p.click("#none")?;
            assert_eq!(p.value("heldOrder().length")?, json!(600));
            p.click("#selmode")?;
            p.click_at("#holdCards .hcard", 0, false)?;
            let last = tray_end(p)?;
            p.click_at(&format!("#holdCards .hcard[data-key=\"{last}\"]"), 0, true)?;
            assert_eq!(p.value("heldPicked().length")?, json!(600));
            Ok(())
        },
    )?;
    let stroke = synth(400, false, "static", None, &[]);
    browser.case("stroke-each-card-once",url,model(&stroke),|p| {
        p.click("#selmode")?;
        let calls=p.value("(()=>{let n=0;const real=markPicked;markPicked=function(){n++;return real.apply(this,arguments)};paintFrom=0;paintTo=true;strokeSpan=[];strokeBase=new Set();const t=performance.now();for(let j=0;j<300;j++)applyStroke(j);const ms=performance.now()-t;markPicked=real;return [n,ms];})()")?;
        println!("PANEL_PERF {}",json!({"scenario":"stroke-300","mark_calls":calls[0],"ms":calls[1]}));assert_eq!(calls[0],300);Ok(())
    })?;
    browser.case("draft-import-undo", url, model(&six), |p| {
        let before = p.value("ITEMS.map(x=>x.key)")?;
        let mut snapshot = p.value("snapshot()")?;
        let mut reverse = snapshot["order"].as_array().unwrap().clone();
        reverse.reverse();
        let wanted = reverse.clone();
        reverse.push(json!("x:gone"));
        snapshot["order"] = json!(reverse);
        p.import_draft(&json!({"version":1,"snapshot":snapshot}))?;
        p.wait(&format!("ITEMS[0].key!=={}", before[0]))?;
        assert_eq!(p.value("ITEMS.map(x=>x.key)")?, json!(wanted));
        assert_eq!(p.value("pendingOrder")?, Value::Null);
        assert!(text(p, "#toast")?.contains("1 not in this catalog"));
        p.click("#undo")?;
        assert_eq!(p.value("ITEMS.map(x=>x.key)")?, before);
        Ok(())
    })?;
    browser.case("draft-unknown-version", url, model(&six), |p| {
        let before = p.value("ITEMS.map(x=>x.key)")?;
        let mut reverse = before.as_array().unwrap().clone();
        reverse.reverse();
        p.import_draft(&json!({"version":2,"snapshot":{"order":reverse}}))?;
        p.wait("document.getElementById('toast').textContent.includes('Not a draft')")?;
        assert_eq!(p.value("ITEMS.map(x=>x.key)")?, before);
        Ok(())
    })?;
    browser.case("drop-first-card-destination",url,model(&packed),|p| {
        hold_first(p)?;let moved=p.value("()=>{const it=ITEMS.find(x=>!x.included&&!x.isLogo);const head=packStarts().starts.find(s=>s.pack===2).index;remember();holdDragKeys=new Set([it.key]);carried.clear();carried.add(it.key);aimAt(head);moveCarried(head);const accepted=acceptDrop();return {accepted,pack:it.pack,at:ITEMS.indexOf(it),head:packStarts().starts.find(s=>s.pack===2).index,runs:packStarts().starts.map(s=>s.pack)};}")?;
        assert_eq!(moved["accepted"],true);assert_eq!(moved["pack"],2);assert_eq!(moved["at"],moved["head"]);assert_eq!(moved["runs"],json!([1,2]));Ok(())
    })?;
    browser.case("grid-drop-restamps",url,model(&packed),|p| {
        let moved=p.value("()=>{const it=ITEMS.find(x=>x.pack===1&&x.included&&!x.isLogo);const mid=packStarts().starts.find(s=>s.pack===2).index+2;dragKey=it.key;dragSnap=snapshot();aimedPack=undefined;carried.clear();carried.add(it.key);aimAt(mid);moveCarried(mid);const accepted=acceptDrop();commitDrag();endDrag(true);return {accepted,pack:it.pack,runs:packStarts().starts.map(s=>s.pack)};}")?;
        assert_eq!(moved["accepted"],true);assert_eq!(moved["pack"],2);assert_eq!(moved["runs"],json!([1,2]));Ok(())
    })?;
    browser.case("drop-unknown-destination",url,model(&packed),|p| {
        hold_first(p)?;let after=p.value("()=>{const it=ITEMS.find(x=>!x.included&&!x.isLogo);const was=it.pack;holdDragKeys=new Set([it.key]);carried.clear();carried.add(it.key);aimedPack=undefined;const accepted=acceptDrop();return {accepted,was,now:it.pack};}")?;
        assert_eq!(after["accepted"],true);assert_eq!(after["now"],after["was"]);Ok(())
    })?;
    browser.case("drop-unnumbered-invents-no-pack",url,model(&twelve),|p| {
        let after=p.value("()=>{const it=ITEMS.find(x=>x.included&&!x.isLogo);it.pack=7;dragKey=it.key;dragSnap=snapshot();aimedPack=undefined;carried.clear();carried.add(it.key);aimAt(6);moveCarried(6);const accepted=acceptDrop();endDrag(true);return {accepted,aimed:aimedPack??null,now:it.pack??null};}")?;
        assert_eq!(after["accepted"],true);assert_eq!(after["aimed"],Value::Null);assert_eq!(after["now"],Value::Null);Ok(())
    })?;
    browser.case("grid-face-shift-range", url, model(&twelve), |p| {
        p.click("#selmode")?;
        p.click_at(".card .thumb", 1, false)?;
        assert_eq!(p.value("picked.size")?, json!(1));
        p.click_at(".card .thumb", 4, true)?;
        assert_eq!(p.value("picked.size")?, json!(4));
        Ok(())
    })?;
    browser.case("grid-box-exact-toggle", url, model(&twelve), |p| {
        p.click("#selmode")?;
        p.click_at(".card .pick", 2, false)?;
        assert_eq!(p.value("picked.size")?, json!(1));
        p.click_at(".card .pick", 2, false)?;
        assert_eq!(p.value("picked.size")?, json!(0));
        Ok(())
    })?;
    browser.case("ordinary-click-holds", url, model(&twelve), |p| {
        p.click_at(".card .thumb", 0, false)?;
        assert_eq!(count(p, "#holdCards .hcard")?, 1);
        assert_eq!(p.value("picked.size")?, json!(0));
        Ok(())
    })?;
    let logo = synth(6, true, "static", None, &[]);
    browser.case("logo-never-picked", url, model(&logo), |p| {
        p.click("#selmode")?;
        p.click(".card.logo")?;
        assert_eq!(p.value("picked.size")?, json!(0));
        Ok(())
    })?;
    browser.case("picked-ring-runs", url, model(&twelve), |p| {
        p.click("#selmode")?;
        p.click_at(".card .thumb", 0, false)?;
        let marked = ring(p, ".card.picked")?;
        let plain = ring(p, ".card:not(.picked)")?;
        assert_eq!(marked["anim"], "pickspin");
        assert!(
            marked["play"]
                .as_array()
                .unwrap()
                .contains(&json!("running"))
        );
        assert_eq!(plain["anim"], "none");
        Ok(())
    })?;
    browser.case("held-ring-matches-grid", url, model(&twelve), |p| {
        hold_first(p)?;
        p.click("#holdCards .hcard img")?;
        picked(p, 1)?;
        assert_eq!(ring(p, ".hcard.picked")?["anim"], "pickspin");
        Ok(())
    })
}
