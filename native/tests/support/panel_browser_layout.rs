//! Original layout, zoom, freeze and preference assertions over the real page and synthetic models.
use super::driver::{Browser, Case, Page};
use pyo3::prelude::*;
use serde_json::{Value, json};

pub fn synth(
    n: usize,
    logo: bool,
    format: &str,
    packs: Option<&[usize]>,
    excluded: &[usize],
) -> Value {
    let mut items = Vec::new();
    if logo {
        items.push(json!({"key":"__brand_logo__","fmt":"static","isLogo":true,
            "label":"Brand logo (auto-added on publish)","emoji":"","included":true}));
    }
    for i in 0..n {
        let mut item = json!({"key":format!("x:{i:030}"),"fmt":format,"label":format!("item {i}"),
            "copyId":null,"emoji":"","included":!excluded.contains(&i)});
        if let Some(packs) = packs {
            item["pack"] = json!(packs[i]);
        }
        items.push(item);
    }
    json!(items)
}
pub fn model(items: &Value) -> Case<'_> {
    Case {
        items: Some(items),
        ..Case::default()
    }
}

fn anchored(page: &Page<'_>, before: &Value) -> PyResult<()> {
    let after = page.value("__anchor()")?;
    let index = before["idx"].as_i64().unwrap();
    if after["atEnd"] == true
        && after["top"]["start"]
            .as_i64()
            .is_some_and(|start| start <= index)
    {
        return Ok(());
    }
    assert!(!after["top"].is_null(), "grid scrolled beyond its rows");
    let start = after["top"]["start"].as_i64().unwrap();
    if after["cols"] == before["cols"] {
        assert_eq!(start, index);
    } else {
        assert!(
            start <= index && index < after["top"]["end"].as_i64().unwrap(),
            "anchor left the top row"
        );
    }
    Ok(())
}
fn barrier(page: &Page<'_>, action: &str) -> PyResult<()> {
    page.eval("(()=>{ window.__tick=0; window.__probe=new IntersectionObserver(()=>{window.__tick++}, {root:null,rootMargin:'0px'}); document.querySelectorAll('#grid img[data-anim]').forEach(n=>__probe.observe(n)); })()")?;
    page.wait("__tick > 0")?;
    page.eval(&format!("(()=>{{window.__tick=0; {action} }})()"))?;
    page.wait("__tick > 0")
}
fn pack_count(page: &Page<'_>) -> PyResult<(u64, u64, String)> {
    let seen = page.value("(()=>{const warn=document.getElementById('capWarn'); const match=warn.textContent.match(/(\\d+) packs/); return [Math.max(1,rows.filter(r=>r.sep).length), match && getComputedStyle(warn).display!=='none' ? +match[1] : 1, warn.textContent];})()")?;
    Ok((
        seen[0].as_u64().unwrap(),
        seen[1].as_u64().unwrap(),
        seen[2].as_str().unwrap().into(),
    ))
}

pub fn run(browser: &Browser<'_>, url: &str) -> PyResult<()> {
    let runs = synth(6, false, "static", Some(&[1, 1, 2, 2, 1, 1]), &[]);
    browser.case("separators-each-run", url, model(&runs), |p| {
        assert_eq!(p.value("rows.filter(r=>r.sep).length")?, json!(3));
        assert_eq!(p.value("document.querySelectorAll('#grid .packsep').length")?, json!(3));
        assert_eq!(p.value("[...grid.children].filter(n=>n.classList.contains('card')||n.classList.contains('packsep')).map(n=>n.classList.contains('packsep')?n.querySelector('span').textContent:'#'+n.querySelector('.pos').textContent)")?,
            json!(["Pack 1","#1","#2","Pack 2","#3","#4","Pack 1","#5","#6"]));
        p.eval("__reorder(5,0)")?;
        assert_eq!(p.value("rows.filter(r=>r.sep).length")?, p.value("document.querySelectorAll('#grid .packsep').length")?);
        Ok(())
    })?;
    browser.case("removed-run-loses-marker", url, model(&runs), |p| {
        p.eval("(()=>{for(const i of [2,3]){ITEMS[i].included=false;setCard(ITEMS[i]);}relayout();updateCount();})()")?;
        assert_eq!(p.value("document.querySelectorAll('#grid .packsep').length")?,p.value("rows.filter(r=>r.sep).length")?);
        Ok(())
    })?;
    let thousand = synth(1000, false, "static", None, &[]);
    browser.case("zoom-anchor-positions", url, model(&thousand), |p| {
        for scroll in [
            "scrollTo(0,0)",
            "scrollTo(0,document.documentElement.scrollHeight/2)",
            "scrollTo(0,document.documentElement.scrollHeight)",
        ] {
            for button in ["#zoomIn", "#zoomOut"] {
                p.eval("setZoom(1)")?;
                p.eval(scroll)?;
                let before = p.value("__anchor()")?;
                assert!(before["idx"].as_i64().unwrap() >= 0);
                p.click(button)?;
                anchored(p, &before)?;
            }
        }
        Ok(())
    })?;
    let forty = synth(40, false, "static", None, &[]);
    browser.case("zoom-reload", url, model(&forty), |p| {
        p.click("#zoomIn")?;
        let zoom = p.value("zoom")?;
        assert!(zoom.as_f64().unwrap() > 1.0);
        p.reload()?;
        assert_eq!(p.value("zoom")?, zoom);
        assert_eq!(
            p.value("document.getElementById('zoomReset').value")?,
            json!(format!("{:.0}%", zoom.as_f64().unwrap() * 100.0))
        );
        Ok(())
    })?;
    let packs = (0..1000).map(|i| 1 + i / 150).collect::<Vec<_>>();
    let thousand_packs = synth(1000, false, "static", Some(&packs), &[]);
    browser.case("zoom-round-trip", url, model(&thousand_packs), |p| {
        p.eval("scrollTo(0,document.documentElement.scrollHeight)")?;
        let before = p.value("__anchor()")?;
        for _ in 0..3 {
            p.click("#zoomIn")?;
        }
        p.double_click("#zoomReset")?;
        assert_eq!(p.value("zoom")?, json!(1));
        anchored(p, &before)
    })?;
    let animated = synth(300, false, "animated", None, &[]);
    browser.case("visible-animation", url, model(&animated), |p| {
        p.wait("__animating()>0")
    })?;
    browser.case("hidden-freeze",url,model(&animated),|p| {
        p.wait("__animating()>0")?;
        p.eval("(()=>{Object.defineProperty(document,'hidden',{configurable:true,get:()=>true});document.dispatchEvent(new Event('visibilitychange'));})()")?;
        assert_eq!(p.value("__animating()")?,json!(0));
        barrier(p,"scrollBy(0,innerHeight*3);")?;
        assert_eq!(p.value("__animating()")?,json!(0)); Ok(())
    })?;
    browser.case("master-freeze", url, model(&animated), |p| {
        p.click("#anim")?;
        assert_eq!(p.value("__animating()")?, json!(0));
        barrier(p, "scrollBy(0,innerHeight*3);")?;
        assert_eq!(p.value("__animating()")?, json!(0));
        Ok(())
    })?;
    browser.case("scroll-freeze", url, model(&animated), |p| {
        p.wait("__animating()>0")?;
        barrier(p, "__keepScrolling(4000);")?;
        let state = p.value("({thawing:scrollThaw!==null,playing:__animating()})")?;
        assert_eq!(
            state["thawing"], true,
            "freeze window ended before the observer proved anything"
        );
        assert_eq!(state["playing"], 0);
        p.wait("__animating()>0")
    })?;
    let video = synth(12, false, "video", None, &[]);
    browser.case(
        "reduced-motion-real-video",
        url,
        Case {
            items: Some(&video),
            reduced_motion: true,
            init: Some("localStorage.setItem('videoReal','1')"),
            ..Case::default()
        },
        |p| {
            p.click("#anim")?;
            p.hover("#grid .thumb", 0)?;
            assert_eq!(p.value("__plays")?, json!(0));
            p.click("#anim")?;
            p.hover("#grid .thumb", 1)?;
            assert!(p.value("__plays")?.as_u64().unwrap() > 0);
            Ok(())
        },
    )?;
    browser.case("reduced-motion-preview",url,Case {items:Some(&video),reduced_motion:true,..Case::default()},|p| {
        p.hover("#grid .thumb",1)?;
        assert_eq!(p.value("(()=>{const n=document.querySelectorAll('#grid .fmt-video img[data-anim]')[1];return n.getAttribute('src')===n.dataset.anim})()")?,json!(true));
        p.pointer_away()?;
        assert_eq!(p.value("(()=>{const n=document.querySelectorAll('#grid .fmt-video img[data-anim]')[1];return n.getAttribute('src')===n.dataset.anim})()")?,json!(false)); Ok(())
    })?;
    browser.case("video-preview-default", url, model(&video), |p| {
        assert_eq!(
            p.value("document.querySelectorAll('#grid video').length")?,
            json!(0)
        );
        assert!(
            p.value("document.querySelectorAll('#grid .fmt-video img[data-anim]').length")?
                .as_u64()
                .unwrap()
                > 0
        );
        p.click("#videoReal")?;
        assert!(
            p.value("document.querySelectorAll('#grid video').length")?
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(
            p.value("document.querySelectorAll('#grid .fmt-video img').length")?,
            json!(0)
        );
        p.key("Control+z")?;
        assert_eq!(
            p.value("document.querySelectorAll('#grid video').length")?,
            json!(0)
        );
        assert_eq!(
            p.value("document.getElementById('videoReal').getAttribute('aria-pressed')")?,
            json!("false")
        );
        Ok(())
    })?;
    for n in [198, 199, 200, 397, 398, 399] {
        let items = synth(n, true, "static", None, &[]);
        browser.case(&format!("pack-count-{n}"), url, model(&items), |p| {
            let (runs, said, warn) = pack_count(p)?;
            assert_eq!(runs, said, "{warn}");
            Ok(())
        })?;
    }
    for (name, items) in [
        ("no-logo-capacity", synth(200, false, "static", None, &[])),
        (
            "excluded-capacity",
            synth(200, true, "static", None, &[0, 1, 2]),
        ),
    ] {
        browser.case(name, url, model(&items), |p| {
            let (runs, said, _) = pack_count(p)?;
            assert_eq!((runs, said), (1, 1));
            Ok(())
        })?;
    }
    let membership = synth(6, false, "static", Some(&[1, 1, 2, 2, 3, 3]), &[]);
    browser.case("live-count-is-estimate", url, model(&membership), |p| {
        assert!(pack_count(p)?.2.to_lowercase().contains("estimate"));
        Ok(())
    })?;
    browser.case("denied-storage",url,Case {init:Some("const boom=()=>{throw new DOMException('site data is blocked','SecurityError')};Object.defineProperty(window,'localStorage',{configurable:true,get:boom});"),..Case::default()},|p| {
        assert_eq!(p.value("document.getElementById('zoomReset').value")?,json!("100%"));
        assert_eq!(p.value("document.getElementById('animLabel').textContent")?,json!("Animation: On"));
        assert_eq!(p.value("document.getElementById('bg').textContent")?,json!("Backdrop: Checker"));
        p.click("#zoomIn")?; assert!(p.value("zoom")?.as_f64().unwrap()>1.0);
        p.click("#anim")?; assert_eq!(p.value("document.getElementById('animLabel').textContent")?,json!("Animation: Off"));
        p.click("#bg")?; assert_eq!(p.value("document.getElementById('bg').textContent")?,json!("Backdrop: Light"));
        let before=p.value("document.getElementById('selCount').textContent")?;
        p.click_card(0)?;assert_ne!(p.value("document.getElementById('selCount').textContent")?,before);
        p.click("#undo")?;assert_eq!(p.value("document.getElementById('selCount').textContent")?,before); Ok(())
    })?;
    browser.case("malformed-preferences",url,Case {init:Some("localStorage.setItem('panelZoom','banana');localStorage.setItem('emojiBg','wat');localStorage.setItem('animOn','maybe');"),..Case::default()},|p| {
        assert_eq!(p.value("zoom")?,json!(1));
        assert_eq!(p.value("document.getElementById('bg').textContent")?,json!("Backdrop: Checker"));
        assert_eq!(p.value("document.getElementById('animLabel').textContent")?,json!("Animation: On")); Ok(())
    })
}
