//! Original real-animation budgets and measured costs; timing is reported, never made a CI threshold.
use super::{
    driver::{Browser, Case, Page},
    layout::{model, synth},
};
use pyo3::prelude::*;
use serde_json::{Value, json};

const PROBE: &str = include_str!("../fixtures/panel-browser-perf.js");
const REFERENCE: &str = include_str!("../fixtures/panel-browser-bulk-reference.js");
fn scenario(items: &Value) -> Case<'_> {
    Case {
        items: Some(items),
        init: Some(PROBE),
        animated: true,
        ..Case::default()
    }
}
fn settle(p: &Page<'_>) -> PyResult<()> {
    p.wait("__settled()")?;
    p.eval("window.__renderSettled=false;requestAnimationFrame(()=>requestAnimationFrame(()=>{window.__renderSettled=true}))")?;
    p.wait("__renderSettled===true")
}
fn compact(p: &Page<'_>) -> PyResult<()> {
    p.eval("setZoom(0.4)")?;
    settle(p)
}
fn percentile(frames: &Value, fraction: f64) -> f64 {
    let mut values = frames
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    (values[((fraction * values.len() as f64) as usize).min(values.len() - 1)] * 10.0).round()
        / 10.0
}
fn scroll_scenario(p: &Page<'_>, zoom: f64) -> PyResult<Value> {
    p.eval(&format!("setZoom({zoom})"))?;
    settle(p)?;
    p.eval("__perf.swaps=0;__perf.loaf=[];window.__flickEnd=performance.now()+1500;__keepScrolling(1500)")?;
    // Observe the actual flick deadline and thaw, not a fixed1600ms sleep or an early scroll gap.
    p.wait("performance.now()>=__flickEnd && __settled() && scrollY>0")?;
    settle(p)?;
    p.eval(
        "window.__sampledFrames=null;__frames(1000).then(value=>{window.__sampledFrames=value})",
    )?;
    p.wait("Array.isArray(__sampledFrames)")?;
    let frames = p.value("__sampledFrames")?;
    assert!(
        frames.as_array().unwrap().len() >= 10,
        "frame probe did not run"
    );
    let perf = p.value("__perf")?;
    let loaf = perf["loaf"].as_array().unwrap();
    let row = json!({"scenario":format!("scroll-{zoom}"),"swaps":perf["swaps"],
        "playing":p.value("__animating()")?,"visible":p.value("__visibleAnimated()")?,
        "loaf_count":loaf.len(),"loaf_blocking_ms":loaf.iter().map(|v|v.as_f64().unwrap()).sum::<f64>(),
        "frame_p50":percentile(&frames,0.5),"frame_p95":percentile(&frames,0.95)});
    println!("PANEL_PERF {row}");
    assert!(
        row["playing"].as_u64().unwrap() >= 1,
        "nothing animates after settling"
    );
    Ok(row)
}

pub fn run(browser: &Browser<'_>, url: &str) -> PyResult<()> {
    let animated = synth(600, false, "animated", None, &[]);
    browser.case("offscreen-never-animates",url,scenario(&animated),|p| {
        for zoom in [1.0,0.4] {
            p.eval(&format!("setZoom({zoom})"))?;settle(p)?;
            assert_eq!(p.value("[...document.querySelectorAll('#grid img[data-anim]')].filter(n=>n.getAttribute('src')===n.dataset.anim).filter(n=>{const r=n.getBoundingClientRect();return !(r.bottom>headerH&&r.top<innerHeight)}).length")?,json!(0));
        }Ok(())
    })?;
    for zoom in [0.4, 1.0] {
        browser.case(
            if zoom < 1.0 {
                "scroll-cost-compact"
            } else {
                "scroll-cost-full"
            },
            url,
            scenario(&animated),
            |p| {
                let row = scroll_scenario(p, zoom)?;
                if zoom < 1.0 {
                    assert!(row["playing"].as_u64().unwrap() <= 24);
                    assert!(
                        row["swaps"].as_u64().unwrap() <= 48,
                        "scroll touched more than the playing budget"
                    );
                }
                Ok(())
            },
        )?;
    }
    browser.case("light-animation-budget", url, scenario(&animated), |p| {
        compact(p)?;
        let visible = p.value("__visibleAnimated()")?.as_u64().unwrap();
        assert!(visible > 24, "fixture did not exceed animation budget");
        assert_eq!(p.value("__animating()")?, json!(visible.min(24)));
        Ok(())
    })?;
    browser.case("budget-nearest-centre",url,scenario(&animated),|p| {
        compact(p)?;
        let distances=p.value("(()=>{const cx=innerWidth/2,cy=headerH+(innerHeight-headerH)/2;const d=n=>{const r=n.getBoundingClientRect();const x=r.left+r.width/2-cx,y=r.top+r.height/2-cy;return x*x+y*y};const vis=[...document.querySelectorAll('#grid img[data-anim]')].filter(n=>{const r=n.getBoundingClientRect();return r.bottom>headerH&&r.top<innerHeight});const on=vis.filter(n=>n.getAttribute('src')===n.dataset.anim).map(d);const off=vis.filter(n=>n.getAttribute('src')!==n.dataset.anim).map(d);return [Math.max(...on),Math.min(...off)];})()")?;
        assert!(distances[0].as_f64().unwrap()<=distances[1].as_f64().unwrap()+1.0);Ok(())
    })?;
    browser.case(
        "all-visible-animation-switch",
        url,
        scenario(&animated),
        |p| {
            compact(p)?;
            p.click("#animAll")?;
            settle(p)?;
            let visible = p.value("__visibleAnimated()")?;
            assert_eq!(p.value("__animating()")?, visible);
            assert!(visible.as_u64().unwrap() > 24);
            p.click("#animAll")?;
            settle(p)?;
            assert!(p.value("__animating()")?.as_u64().unwrap() <= 24);
            Ok(())
        },
    )?;
    let three_hundred = synth(300, false, "static", None, &[]);
    let counter = "window.__varWrites=0;const _set=CSSStyleDeclaration.prototype.setProperty;CSSStyleDeclaration.prototype.setProperty=function(name){if(String(name).startsWith('--'))window.__varWrites++;return _set.apply(this,arguments)};";
    browser.case("drag-css-no-redundant-writes",url,Case {init:Some(counter),..model(&three_hundred)},|p| {
        p.eval("__varWrites=0")?;
        let ms=p.value("(()=>{const t=performance.now();for(let i=0;i<30;i++)__reorder(i,i+5);return performance.now()-t;})()")?;
        let writes=p.value("__varWrites")?;
        println!("PANEL_PERF {}",json!({"scenario":"reorder-30","ms":ms,"var_writes":writes}));
        assert_eq!(writes,json!(0));Ok(())
    })?;
    let thousand = synth(1000, false, "static", None, &[]);
    let count = "(()=>{const cards=[...document.querySelectorAll('#grid .card')];const seen=cards.filter(c=>{const r=c.getBoundingClientRect();return r.bottom>headerH&&r.top<innerHeight});return [cards.length,seen.length,G.cols,document.querySelectorAll('#grid .card .lbl').length];})()";
    browser.case("compact-grid-window",url,Case {init:Some(PROBE),..model(&thousand)},|p| {
        let full=p.value(count)?;compact(p)?;let compact=p.value(count)?;
        println!("PANEL_PERF {}",json!({"scenario":"mounted","zoom1":full[0],"zoom0.4":compact[0],"visible0.4":compact[1]}));
        assert!(compact[0].as_u64().unwrap()<=compact[1].as_u64().unwrap()+6*compact[2].as_u64().unwrap());
        assert_eq!(compact[3],0);p.eval("setZoom(1)")?;settle(p)?;
        assert!(p.value(count)?[3].as_u64().unwrap()>0);Ok(())
    })?;
    browser.case("picked-ring-scroll-pause",url,scenario(&animated),|p| {
        compact(p)?;p.click("#selmode")?;p.click("#all")?;
        let state="getComputedStyle(document.querySelector('#grid .card.picked'),'::before').animationPlayState";
        p.wait(&format!("{state}==='running'"))?;p.eval("__keepScrolling(1000)")?;
        p.wait(&format!("{state}==='paused'"))?;settle(p)?;p.wait(&format!("{state}==='running'"))
    })?;
    browser.case("checkerboard-pixels", url, scenario(&animated), |p| {
        for i in 0..3 {
            let got = p.screenshot_pixels("#grid .thumb", i)?;
            for (colour, want) in
                got.iter()
                    .zip([(130, 140, 154), (70, 78, 90), (70, 78, 90), (130, 140, 154)])
            {
                for (actual, expected) in [colour.0, colour.1, colour.2]
                    .into_iter()
                    .zip([want.0, want.1, want.2])
                {
                    assert!(
                        i16::from(actual).abs_diff(expected) <= 2,
                        "checkerboard pixel changed: {got:?}"
                    );
                }
            }
        }
        Ok(())
    })?;
    let packs = (0..1000).map(|i| 1 + i / 200).collect::<Vec<_>>();
    let bulk = synth(1000, false, "static", Some(&packs), &[]);
    browser.case("bulk-hold-release-cost",url,Case {init:Some(PROBE),..model(&bulk)},|p| {
        let hold=p.value("(()=>{const t=performance.now();document.getElementById('none').click();return performance.now()-t})()")?;
        let held=p.value("ITEMS.filter(x=>!x.isLogo&&!x.included).length")?;
        let release=p.value("(()=>{const t=performance.now();document.getElementById('unholdAll').click();return performance.now()-t})()")?;
        println!("PANEL_PERF {}",json!({"scenario":"bulk-1000","deselect_all_ms":hold,"unhold_all_ms":release}));
        assert_eq!(held,json!(1000));assert_eq!(p.value("ITEMS.filter(x=>!x.isLogo&&!x.included).length")?,json!(0));Ok(())
    })?;
    let four = synth(4, false, "static", None, &[]);
    browser.case(
        "bulk-item-reference-equivalence",
        url,
        Case {
            init: Some(REFERENCE),
            ..model(&four)
        },
        |p| {
            assert_eq!(p.value("__bulkEquivalence(150,20260929)")?, json!([]));
            Ok(())
        },
    )?;
    browser.case(
        "undo-animation-all-visible",
        url,
        scenario(&animated),
        |p| {
            compact(p)?;
            p.click("#animAll")?;
            assert_eq!(
                p.value("document.getElementById('animAll').getAttribute('aria-pressed')")?,
                json!("true")
            );
            p.key("Control+z")?;
            assert_eq!(
                p.value("document.getElementById('animAll').getAttribute('aria-pressed')")?,
                json!("false")
            );
            Ok(())
        },
    )
}
