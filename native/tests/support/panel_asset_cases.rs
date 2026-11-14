//! Original served-text assertions are syntax evidence, separate from real browser acceptance.
use pyo3::prelude::*;
use std::path::Path;

fn block<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let at = text
        .find(start)
        .unwrap_or_else(|| panic!("missing original marker {start}"));
    let tail = &text[at..];
    &tail[..tail
        .find(end)
        .unwrap_or_else(|| panic!("missing original end {end}"))]
}
fn prefix(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}
fn has(text: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            text.contains(pattern),
            "missing original contract {pattern}"
        );
    }
}
fn absent(text: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            !text.contains(pattern),
            "forbidden original contract {pattern}"
        );
    }
}
fn before(text: &str, left: &str, right: &str) {
    assert!(
        text.find(left).unwrap() < text.find(right).unwrap(),
        "{left} must precede {right}"
    );
}

fn actions(page: &str, script: &str) {
    let drop = block(
        script,
        "grid.addEventListener('drop'",
        "grid.addEventListener('dragend'",
    );
    has(drop, &["commitDrag()"]);
    absent(drop, &["findIndex", "splice", "ITEMS.length"]);
    let commit = block(script, "function commitDrag(){", "function endDrag");
    has(
        commit,
        &[
            "order.every((k,i)=>k===dragSnap.order[i])",
            "selSig(ITEMS.filter(x=>x.included).map(x=>x.key))===selSig(dragSnap.included)",
            "packs===JSON.stringify(dragSnap.packs)) return;",
        ],
    );
    before(commit, "remember(dragSnap);", "saveOrder();");
    has(
        block(
            script,
            "grid.addEventListener('dragstart'",
            "grid.addEventListener('dragover'",
        ),
        &["dragSnap=snapshot();"],
    );
    let end = block(script, "function endDrag(committed){", "// --- Auto-scroll");
    has(
        end,
        &["if(!committed && dragKey !== null){ applySnapshot(dragSnap,false); }"],
    );
    has(
        block(
            script,
            "grid.addEventListener('dragover'",
            "function moveCarried",
        ),
        &[
            "getBoundingClientRect",
            "(e.clientX > r.left + r.width/2) ? j + 1 : j",
        ],
    );
    let moved = block(
        script,
        "function moveCarried(slot){",
        "grid.addEventListener('drop'",
    );
    has(moved, &["if(next.every((x,i)=>x===ITEMS[i])) return;"]);
    before(moved, "return;", "relayout();");
    has(script, &["function edgeScroll(", "requestAnimationFrame"]);
    has(
        &prefix(
            &script[script.find("document.addEventListener('dragover'").unwrap()..],
            300,
        ),
        &["edgeScroll(e.clientY)"],
    );
    has(
        block(script, "function retire(node){", "function relayout"),
        &[
            "if(carried.has(node.dataset.key)) park.appendChild(node);",
            "else unmountCard(node);",
        ],
    );
    has(page, &["id=\"park\""]);
    has(
        end,
        &["while(park.firstChild) unmountCard(park.firstChild);"],
    );
    has(
        block(script, "function makeCard(it){", "function packStarts"),
        &["hdr.appendChild(el('span','pos',''));"],
    );
    absent(script, &["el('span','pos', n"]);
    let render = block(script, "function render(){", "function retire");
    has(
        render,
        &["if(pos.textContent !== String(displayPos[k])) pos.textContent = displayPos[k];"],
    );
    let header = block(
        script,
        "const hdr = el('div','hdr');",
        "card.appendChild(hdr);",
    );
    has(
        header,
        &[
            "hdr.appendChild(el('span','pos',''));",
            "if(!it.isLogo) hdr.appendChild(el('span','tick'",
        ],
    );
    absent(render, &["isLogo"]);
    let count = block(script, "function updateCount(){", "\n}");
    has(
        count,
        &["packStarts().starts.length", "PER_SET - (logo ? 1 : 0)"],
    );
    absent(script, &["Math.ceil(included / PER_SET)"]);
    has(page, &["__PER_SET__"]);
    let rule = block(page, ".hdr{", "}");
    has(
        rule,
        &[
            "display:flex",
            "flex-direction:column",
            "align-items:center",
        ],
    );
    for class in [".badge{", ".pos{"] {
        absent(block(page, class, "}"), &["position:absolute"]);
    }
    has(block(page, ".tick{", "}"), &["position:absolute"]);
    let edge = &script[script.find("function stepEdge(").unwrap()..];
    let first_brace = edge.find('}').unwrap();
    let chars = edge[..first_brace].chars().count() + 401;
    has(
        &prefix(edge, chars),
        &["if(dragKey === null || !edgeSpeed) return;"],
    );
    has(
        script,
        &["function stopEdgeScroll(", "cancelAnimationFrame"],
    );

    for (start, end, mutation, remember) in [
        (
            "function setAll(fn){",
            "// Reduced motion",
            "setIncludedMany(",
            "remember()",
        ),
        (
            "grid.addEventListener('click'",
            "// Plain document scrolling",
            "setIncluded(ITEMS[i], !ITEMS[i].included)",
            "remember()",
        ),
        (
            "function commitDrag(){",
            "function endDrag",
            "saveOrder();",
            "remember(dragSnap)",
        ),
    ] {
        before(block(script, start, end), remember, mutation);
    }
    has(script, &["HISTORY_MAX"]);
    let remember = &script[script.find("function remember(snap){").unwrap()..];
    let length = remember[..remember.find('}').unwrap()].chars().count() + 201;
    has(&prefix(remember, length), &["past.shift()"]);
    has(
        &remember[..remember.find("updateHistoryButtons").unwrap()],
        &["future.length = 0"],
    );
    has(
        block(script, "function applySnapshot(", "function undo()"),
        &["relayout();", "saveOrder()"],
    );
    absent(script, &["cards.clear()"]);
    has(
        render,
        &[
            "if(n === cur){ cur = cur.nextSibling; continue; }",
            "grid.insertBefore(n, cur);",
        ],
    );
    let colours = ["static", "animated", "video"].map(|format| {
        let start = format!(".card.fmt-{format}");
        let rule = &page[page.find(&start).unwrap()..];
        &rule[rule.find("--fmt:").unwrap() + 6..rule.find(';').unwrap()]
    });
    assert_eq!(
        colours
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    for colour in colours {
        absent(block(page, ".card.drag{", "}"), &[colour]);
    }
    let thumb = block(page, ".thumb{", "}");
    has(thumb, &["outline:.17em solid var(--fmt", "outline-offset:"]);
    absent(&thumb.replace("border-radius", ""), &["border:"]);
    has(block(page, ".card.on{", "}"), &["#22c55e"]);
    has(block(page, ".tick{", "}"), &["cursor:pointer"]);
    has(script, &["function freezeAll(){"]);
    let scroll = block(script, "addEventListener('scroll'", "}, {passive:true});");
    has(scroll, &["freezeAll()", "applyAnim()", "scheduleRender()"]);
    has(
        &prefix(
            &script[script.find("addEventListener('scroll'").unwrap()..],
            600,
        ),
        &["passive"],
    );
    has(
        block(script, "const animIO", "function setPlaying"),
        &["}, {root: null, rootMargin: '0px'})"],
    );
    has(
        page,
        &[
            "aria-pressed",
            ".switch[aria-pressed=\"true\"]  .knob{background:#22c55e}",
            ".switch[aria-pressed=\"false\"] .knob{background:#f43f5e}",
            "id=\"anim\" class=\"switch\"",
            "id=\"animAll\" class=\"switch\"",
            "id=\"selmode\" class=\"switch\"",
            "--btn:#34ebc6",
        ],
    );
    has(
        block(page, ".hdr{", "}"),
        &["flex-direction:column", "align-items:center"],
    );
    before(header, "'pos'", "'badge'");
    let toolbar = block(page, "<div class=\"actions\">", "</div>");
    for button in [
        "undo",
        "redo",
        "top",
        "bot",
        "zoomOut",
        "zoomReset",
        "zoomIn",
        "all",
        "none",
        "inv",
        "bg",
        "anim",
        "animAll",
        "selmode",
    ] {
        has(toolbar, &[&format!("id=\"{button}\"")]);
    }
    absent(toolbar, &["id=\"save\""]);
    has(page, &["grid-template-columns:auto minmax(0,1fr) auto"]);

    has(script, &["el('span','pick'", "el('span','tick'"]);
    has(
        block(page, ".pick{", "}"),
        &["display:none", "border:", "background:#0b1422"],
    );
    has(page, &["body.selmode .pick{display:flex}"]);
    assert!(
        ["el('span','pick')", "el('span','pick');"]
            .iter()
            .any(|built| script.contains(built))
    );
    absent(script, &["el('span','pick','✓')", "el('span','pick', '✓')"]);
    let picked = ".card.picked .pick::after,.hcard.picked .pick::after";
    has(page, &[picked]);
    has(block(page, &format!("{picked}{{"), "}"), &["✓"]);
    has(script, &["if(!selMode) clearPicked();"]);
    let clicked = block(
        script,
        "grid.addEventListener('click'",
        "// Plain document scrolling",
    );
    has(
        clicked,
        &[
            "if(selMode){",
            "pickCardAt(i, e.shiftKey);",
            "if(!e.target.closest('.pick')) pickCardAt",
        ],
    );
    before(clicked, "if(selMode){", "remember();");
    let ring = ".card.picked::before,.hcard.picked::before";
    has(
        page,
        &[ring, "@keyframes pickspin{to{transform:rotate(1turn)}}"],
    );
    has(
        block(page, &format!("{ring}{{"), "}"),
        &["conic-gradient", "animation:pickspin"],
    );
    absent(
        page,
        &[".card.picked{outline:2px solid #f0abfc;outline-offset:-2px}"],
    );
    has(
        block(page, "@media (prefers-reduced-motion:reduce){", "}}"),
        &[".card.picked::before,.hcard.picked::before{animation:none"],
    );
    has(
        page,
        &[
            "body.moving .card.picked::before,body.moving .hcard.picked::before{animation-play-state:paused}",
        ],
    );
    has(
        block(script, "addEventListener('scroll'", "{passive:true});"),
        &["markMoving();"],
    );
    let start = block(
        script,
        "grid.addEventListener('dragstart'",
        "grid.addEventListener('dragover'",
    );
    has(
        start,
        &[
            "picked.has(dragKey)",
            "ITEMS.filter(x=>picked.has(x.key) && !x.isLogo)",
            "[dragKey]",
        ],
    );
    absent(start, &["querySelectorAll('.card.picked')"]);
    has(script, &["if(node) node.classList.toggle('picked', on);"]);
    has(
        block(script, "function mount(k){", "function unmountCard"),
        &["c.classList.toggle('picked', picked.has(it.key));"],
    );
    has(
        block(script, "function applyStroke(", "\n}"),
        &["strokeBase.has(", "if(!now.has(k))"],
    );
}

fn grid(page: &str, script: &str, root: &Path) {
    has(
        block(script, "const animIO", "}, {root:"),
        &[
            "e.isIntersecting",
            "setPlaying(t, live)",
            "inView.delete(t)",
            "showStill(t)",
            "inView.add(t)",
        ],
    );
    has(
        block(script, "function allocate(){", "\n}"),
        &["showMotion(", "ANIM_BUDGET"],
    );
    has(
        block(script, "function mount(k){", "function unmountCard"),
        &["animIO.observe(n)", "videoIO.observe(n)"],
    );
    has(
        block(script, "function unmountCard(c){", "const videoIO"),
        &[
            "animIO.unobserve(n)",
            "forgetNode(n)",
            "videoIO.unobserve(n)",
            "cards.delete(c.dataset.key);",
        ],
    );
    has(
        block(
            script,
            "if(it.fmt === 'video' && !VIDEO_REAL){",
            "} else if(it.fmt === 'video'){",
        ),
        &["previewSources("],
    );
    let video = block(
        script,
        "} else if(it.fmt === 'video'){",
        "} else if(it.fmt === 'animated'){",
    );
    has(video, &["v.dataset.src = src", "v.poster="]);
    absent(video, &["v.src ="]);
    has(
        block(
            script,
            "function attachVideo(v, on){",
            "function setPlaying",
        ),
        &[
            "v.src = v.dataset.src",
            "v.removeAttribute('src'); v.load();",
        ],
    );
    has(
        block(script, "const videoIO", "function attachVideo"),
        &["rootMargin: '0px'"],
    );
    has(
        block(script, "function setPlaying(v, on){", "}"),
        &["if (on) attachVideo(v, true);"],
    );
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let re = py.import("re")?;
        let options = pyo3::types::PyDict::new(py);
        options.set_item("flags", re.getattr("S")?)?;
        let header: String = re
            .call_method(
                "sub",
                (r"/\*.*?\*/", "", block(page, "header{", "}")),
                Some(&options),
            )?
            .extract()?;
        absent(&header, &["backdrop-filter"]);
        assert_eq!(
            re.call_method1("findall", (r"localStorage\.(?:get|set)Item", script))?
                .len()?,
            2
        );
        Ok(())
    })
    .unwrap();
    let render = block(script, "function render(){", "function retire");
    has(
        render,
        &[
            "const pad = Math.min(innerHeight / 2, BUFFER_ROWS * (G.cardH + G.gap));",
            "let first = rowAt(viewTop - pad);",
            "setSpacer(padTop, first > 0 ? rows[first].top - G.gap : 0);",
            "setSpacer(padBot, bottom < total ? total - bottom - G.gap : 0);",
        ],
    );
    has(script, &["const BUFFER_ROWS = 3;"]);
    has(
        page,
        &["id=\"padTop\"", "id=\"padBot\"", ".spacer{grid-column:1/-1"],
    );
    let layout = block(script, "function layoutRows(){", "function measure");
    has(
        layout,
        &[
            "Math.round(BASE.gap * zoom)",
            "Math.round((compact ? BASE.cardHCompact : BASE.cardH) * zoom)",
            "setVar('--cardH', G.cardH + 'px');",
            "if(sepAt.has(i)){",
            "indices.length < G.cols && !sepAt.has(visible[v])",
            "win = {first: -1, last: -1};",
        ],
    );
    has(
        block(page, ".card{", "}"),
        &["height:var(--cardH", "overflow:hidden"],
    );
    has(block(page, "\n.lbl{", "}"), &["-webkit-line-clamp:2"]);
    absent(page, &["content-visibility", "contain-intrinsic-size"]);
    has(
        render,
        &[
            "if(first === win.first && last === win.last) return;",
            "if(n === cur){ cur = cur.nextSibling; continue; }",
            "grid.insertBefore(n, cur);",
            "retire(stale)",
        ],
    );
    has(
        block(script, "function scheduleRender(){", "}"),
        &["requestAnimationFrame"],
    );
    has(
        page,
        &[
            "id=\"zoomOut\"",
            "id=\"zoomReset\"",
            "id=\"zoomIn\"",
            "<input id=\"zoomReset\"",
        ],
    );
    has(
        script,
        &[
            "document.getElementById('zoomIn').onclick = ()=>setZoom(zoom * ZOOM_STEP);",
            "document.getElementById('zoomOut').onclick = ()=>setZoom(zoom / ZOOM_STEP);",
            "if(e.key==='Enter'){e.preventDefault();applyZoomInput();}",
            "zoomInput.addEventListener('dblclick',()=>setZoom(1));",
        ],
    );
    has(
        block(script, "addEventListener('wheel'", "{passive: false});"),
        &["if(!e.ctrlKey) return;", "e.preventDefault();"],
    );
    has(
        block(script, "addEventListener('keydown', e=>{", "});"),
        &["'='", "'-'", "'0'"],
    );
    let zoom = block(script, "function setZoom(z){", "function paintZoom");
    has(
        zoom,
        &[
            "Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z))",
            "prefs.set('panelZoom'",
        ],
    );
    has(
        block(script, "function loadZoom(){", "}"),
        &["prefs.get('panelZoom'"],
    );
    has(script, &["const prefs = {"]);
    before(zoom, "const anchor = firstVisibleIndex();", "zoom = z;");
    before(
        zoom,
        "layoutRows();",
        "scrollTo(0, gridTop + rows[rowOfItem[anchor]].top - headerH)",
    );
    has(
        script,
        &[
            "const COMPACT_BELOW = 0.75;",
            "document.body.classList.toggle('compact', compact);",
        ],
    );
    has(
        page,
        &["body.compact .glyph,body.compact .lbl,body.compact .sub{display:none}"],
    );
    has(block(page, ".card{", "}"), &["font-size:calc(12px*var(--z"]);
    has(block(page, ".thumb{", "}"), &["width:9em;height:9em"]);
    has(
        block(page, ".thumb img,.thumb video{", "}"),
        &["width:8.67em;height:8.67em"],
    );
    absent(page, &["104px"]);
    absent(script, &["img.width = 104"]);
    let asset = root.join("assets/panel.html");
    assert!(asset.is_file());
    assert!(
        asset
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.trim_end().ends_with("</html>"));
    has(
        page,
        &[
            "__ITEMS__",
            "__TOKEN__",
            "__PREVIEW_TIERS__",
            "__PER_SET__",
            "__HIDDEN__",
            "__STALE__",
            "__ASSET_VER__",
            "__ICON_VER__",
        ],
    );
    has(block(page, ".packsep{", "}"), &["grid-column:1/-1"]);
    let separator = block(script, "function sepNode(", "\nfunction ");
    has(separator, &["el('div','packsep')"]);
    absent(separator, &["'card'"]);
    absent(script, &["packsep card", "'Not in a pack yet'"]);
    has(script, &["e.target.closest('.card')"]);
    let packs = block(script, "function packStarts(){", "function layoutRows");
    has(
        packs,
        &[
            "!it.isLogo && it.included",
            "PER_SET - (logo ? 1 : 0)",
            "pk !== null && (!starts.length || pk !== cur)",
        ],
    );
    before(
        packs,
        "if(byMembership){",
        "} else if(!it.isLogo && it.included){",
    );
    absent(
        block(
            packs,
            "if(byMembership){",
            "} else if(!it.isLogo && it.included){",
        ),
        &["isLogo"],
    );
    has(
        script,
        &[
            "relayout(); updateCount(); markSelDirty(); }",
            "lastIdx=i; relayout(); updateCount(); markSelDirty();",
        ],
    );
    has(
        layout,
        &[
            "const sepAt = new Map();",
            "rows.push({sep: sepAt.get(i), at: i, top: y, h: G.sepH});",
            "if(starts.length >= 2){",
        ],
    );
    has(page, &["id=\"top\"", "id=\"bot\""]);
    let jump = prefix(
        &script[script
            .find("document.getElementById('top').onclick")
            .unwrap()..],
        400,
    );
    has(
        &jump,
        &[
            "window.scrollTo({top:0})",
            "document.documentElement.scrollHeight",
        ],
    );
    absent(&jump, &["scrollIntoView"]);
}

pub fn check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert_eq!(
        _native::asset_version::versions(root).unwrap(),
        ("23a5e175b383".into(), "4636a2cbdf7a".into())
    );
    let page = std::fs::read_to_string(root.join("assets/panel.html"))
        .unwrap()
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let script = [
        "panel-grid.js",
        "panel-motion.js",
        "panel-save.js",
        "panel-drag.js",
        "panel-actions.js",
        "panel-holding.js",
        "panel-draft.js",
    ]
    .iter()
    .map(|name| {
        std::fs::read_to_string(root.join("assets").join(name))
            .unwrap()
            .replace("\r\n", "\n")
            .replace('\r', "\n")
    })
    .collect::<Vec<_>>()
    .join("\n");
    actions(&page, &script);
    grid(&page, &script, root);
}
