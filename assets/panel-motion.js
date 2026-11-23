// panel-motion.js -- which cards may animate, and when. Loaded right after
// panel-grid.js; mount()/unmountCard() there observe and release the nodes.

// A <video> costs a media player from the moment it has a source, and creating
// or tearing one down is the most expensive thing a card can do on the main
// thread -- measured as the 60-140 ms frames that were left once the grid was
// virtual. So a mounted video card carries only its URL; the source is attached
// when the card comes within a row of the viewport and released when it leaves.
const videoIO = window.IntersectionObserver ? new IntersectionObserver(es => {
  for (const e of es) attachVideo(e.target, e.isIntersecting);
}, {root: null, rootMargin: '0px'}) : null;

// Players are neither created nor torn down mid-scroll: each costs a 60-140 ms
// frame, which is exactly the stutter a scroll is trying to avoid. A release
// requested meanwhile is queued and done once the scroll settles.
const pendingDetach = new Set();
function releasePendingDetach(){
  for (const v of pendingDetach) {
    const r = v.getBoundingClientRect();
    if (!v.isConnected || r.bottom <= 0 || r.top >= innerHeight || document.hidden || !ANIM_ON ||
        (ANIM_HOVER && v !== previewVideo)) attachVideo(v, false);
  }
  pendingDetach.clear();
}

function attachVideo(v, on){
  if (on && (dragging || windowBlurred || movingTimer !== null || (ANIM_HOVER && v !== previewVideo))) return;
  on=on && ANIM_ON && !document.hidden;
  if (scrollThaw !== null) { if (!on && v.getAttribute('src')) pendingDetach.add(v); return; }
  try {
    if (on) { if (!v.getAttribute('src')) v.src = v.dataset.src; }
    else if (v.getAttribute('src')) { v.pause(); v.removeAttribute('src'); v.load(); }
  } catch(_){}
}

// ---- animation gating ----------------------------------------------------
// Only the cards you can actually see animate. Everything else holds frame 0,
// so the number of live animations is bounded by the viewport rather than by
// the catalog.
/** May this node animate right now? The ONE answer, asked by every play,
 *  source swap, observer callback, thaw and hover path.
 *
 *  Eligibility used to be intersection plus the master switch, so a callback
 *  delivered after a freeze restarted exactly what the freeze had stopped: the
 *  observer knew nothing about a hidden tab or a scroll in progress, and those
 *  are the two moments the freeze exists for.
 *
 *  Hover only and reduced motion allow interaction without starting idle cards. */
let windowBlurred = false;
function mayAnimate(inView, hover){
  if(!ANIM_ON || document.hidden || windowBlurred || dragging ||
     movingTimer !== null || scrollThaw !== null) return false;
  return hover ? true : (inView && !RM && !ANIM_HOVER);
}

// Swapping an <img> src is cheap -- both URLs are immutable-cached, so this
// never refetches -- which is what makes this affordable where
// mounting/destroying a player was not.
const animIO = window.IntersectionObserver ? new IntersectionObserver(es => {
  for (const e of es) {
    const t = e.target;
    const live = mayAnimate(t.isConnected && e.isIntersecting && e.boundingClientRect.bottom>headerH);
    if (t.dataset.play) {
      if (ANIM_HOVER || RM) scheduleAllocate();
      else setPlaying(t, live);
      continue;
    }
    // Images only report what is on screen; allocate() decides which of them
    // may move, so a burst of callbacks costs one decision per frame.
    if (t.isConnected && e.isIntersecting && e.boundingClientRect.bottom > headerH) inView.add(t);
    else { inView.delete(t); showStill(t); }
    scheduleAllocate();
  }
}, {root: null, rootMargin: '0px'}) : null;   // see freezeAll(): a band
// beyond the viewport animated a row nobody was looking at, above AND below.

// Owner decision 2026-09-28: at most this many cards animate at once, chosen
// nearest the centre of the screen. Every visible animated WebP decodes on the
// main thread's budget, and at low zoom that was 128 of them per frame -- the
// CPU jump the owner saw the moment Animation was switched on. "All visible"
// lifts the cap for whoever wants the old behaviour.
const ANIM_BUDGET = 24;
let ANIM_ALL = prefs.get('animAll', '0') === '1';
let ANIM_HOVER = prefs.get('animHover', '0') === '1';
const inView = new Set();    // animated <img> intersecting the viewport
const playing = new Set();   // <img> currently showing their animated source
function showStill(n){ playing.delete(n);
  if (n.getAttribute('src') !== n.dataset.still) n.src = n.dataset.still; }
function showMotion(n){ playing.add(n);
  if (n.getAttribute('src') !== n.dataset.anim) n.src = n.dataset.anim; }
function forgetNode(n){
  inView.delete(n); playing.delete(n);
  if (n === previewVideo) previewVideo = null;
}

let hovered = null, focused = null, focusLast = false, previewVideo = null;
function previewNode(){
  if (!mayAnimate(false, true)) return null;
  let selector = null;
  if (ANIM_HOVER) selector = '.card';
  else if (RM) {
    selector = '.fmt-video .thumb';
  } else return null;
  const from = target => grid.contains(target) && target.closest(selector);
  const card = focusLast ? from(focused) || from(hovered) : from(hovered) || from(focused);
  const n = card && card.querySelector('img[data-anim],video[data-play]');
  if (!n) return null;
  const r = n.getBoundingClientRect();
  return r.bottom > headerH && r.top < innerHeight ? n : null;
}
function previewVideoOnly(v){
  if (v !== previewVideo && previewVideo) {
    setPlaying(previewVideo, false);
    if (ANIM_HOVER) attachVideo(previewVideo, false);
    else try { previewVideo.currentTime = 0; } catch(_){}
  }
  previewVideo = v;
  if (v?.paused) setPlaying(v, true);
}
for (const [enter, leave, focus] of /** @type {const} */ ([['mouseover', 'mouseout', false], ['focusin', 'focusout', true]])) {
  for (const type of [enter, leave]) grid.addEventListener(type, e => {
    const target = type === enter ? e.target : e.relatedTarget;
    const next = target instanceof Element && grid.contains(target) ? target : null;
    const previous = focus ? focused : hovered;
    if (focus) focused = next; else hovered = next;
    if (!(ANIM_HOVER || RM)) return;
    const selector = ANIM_HOVER ? '.card' : '.fmt-video .thumb';
    if (previous?.closest(selector) === next?.closest(selector)) return;
    if (next?.closest(selector)) focusLast = focus;
    allocate();
  });
}

let allocRaf = null;
function scheduleAllocate(){ if (allocRaf === null) allocRaf = requestAnimationFrame(allocate); }
function allocate(){
  if (allocRaf !== null) cancelAnimationFrame(allocRaf);
  allocRaf = null;
  let live = [];
  const preview = previewNode();
  if (preview instanceof HTMLImageElement) live = [preview];
  else if (mayAnimate(true)) {
    // Read every rectangle first, then write: one layout, not one per card.
    // The rectangle is re-read here rather than trusted from the observer: a
    // card that slid under the sticky header is still "intersecting".
    const cx = innerWidth / 2, cy = headerH + (innerHeight - headerH) / 2;
    const seen = [];
    for (const n of inView) {
      if (!n.isConnected) { forgetNode(n); continue; }
      const r = n.getBoundingClientRect();
      if (r.bottom <= headerH || r.top >= innerHeight) continue;
      const dx = r.left + r.width / 2 - cx, dy = r.top + r.height / 2 - cy;
      seen.push([dx * dx + dy * dy, n]);
    }
    if (!ANIM_ALL && seen.length > ANIM_BUDGET) seen.sort((a, b) => a[0] - b[0]).length = ANIM_BUDGET;
    live = seen.map(p => p[1]);
  }
  const keep = new Set(live);
  for (const n of [...playing]) if (!keep.has(n)) showStill(n);
  if (!dragging && movingTimer === null && scrollThaw === null) {
    previewVideoOnly(preview instanceof HTMLVideoElement ? preview : null);
  }
  for (const n of live) if (!playing.has(n)) showMotion(n);
}

// play() rejects when the element is detached or the browser refuses; that is
// not an error worth surfacing, but it MUST be caught or it becomes an
// unhandled rejection on every scroll.
function setPlaying(v, on){
  // The guard sits HERE, where every caller already routes, rather than at each
  // of them: one path that forgot to ask was all it took to undo a freeze.
  if (on) on = v.isConnected && mayAnimate(true, true);
  if (on) attachVideo(v, true);      // playing implies a source, whichever observer spoke first
  try { if (on) { const q = v.play(); if (q) q.catch(()=>{}); } else { v.pause(); } }
  catch(_){}
}

/** Hold every mounted card on frame 0. Costs no request: the still is the
 *  same immutable-cached URL the card was built with. */
function freezeAll(){
  grid.querySelectorAll('video[data-play]').forEach(v => {setPlaying(v,false);if(document.hidden||!ANIM_ON)attachVideo(v,false);});
  // Only what is playing needs stopping: the rest already shows its still.
  for (const img of [...playing]) showStill(img);
}

window.addEventListener('blur', () => { windowBlurred = true; freezeAll(); });
window.addEventListener('focus', () => { windowBlurred = false; if (!document.hidden) applyAnim(); });

// Switching to another tab or window used to change nothing: every visible
// animation kept decoding for a page nobody was looking at. The browser
// throttles rAF for a hidden tab but not image animation, so this has to be
// explicit. Coming back re-evaluates visibility rather than assuming.
document.addEventListener('visibilitychange', ()=>{
  if (document.hidden) freezeAll(); else applyAnim();
});

// Scrolling is when a grid feels heavy, and it is the one moment the work is
// pure waste: the compositor is already busy, every visible animated WebP keeps
// decoding, and the frames go past too fast to see. Freeze on the first scroll
// event, thaw once it settles; the render itself is rAF-throttled.
let scrollThaw = null;
// `body.moving` pauses the always-running decoration (the pick ring) while the
// page scrolls or a card is dragged, and lets it run again once things settle.
let movingTimer = null, dragging = false;
function markMoving(){
  document.body.classList.add('moving');
  clearTimeout(movingTimer);
  movingTimer = setTimeout(()=>{
    movingTimer = null;
    if (!dragging) {
      document.body.classList.remove('moving');
      if (scrollThaw === null && ANIM_ON) applyAnim();
    }
  }, 180);
}
document.addEventListener('dragstart', e => {
  if (e.defaultPrevented) return;
  dragging = true;
  document.body.classList.add('moving');
  freezeAll();
});
for (const type of ['dragend', 'drop']) {
  document.addEventListener(type, ()=>{ if (dragging) { dragging = false; markMoving(); } }, true);
}

addEventListener('scroll', ()=>{
  scheduleRender();
  markMoving();
  // Not `|| RM`: under reduced motion a hovered video is the one thing that
  // can be playing, and `scrollThaw` is how mayAnimate() knows to hold it.
  if(!ANIM_ON) return;
  if(scrollThaw === null) freezeAll();
  else clearTimeout(scrollThaw);
  scrollThaw = setTimeout(()=>{ scrollThaw = null; releasePendingDetach(); applyAnim(); }, 180);
}, {passive:true});

// Master switch. Off = every card holds frame 0 and nothing decodes at all,
// which is the lightest the grid can be; the observer stops swapping so it
// cannot undo the freeze behind your back.
let ANIM_ON = prefs.get('animOn', '1') !== '0';
function applyAnim(){
  const btn = document.getElementById('anim');
  btn.setAttribute('aria-pressed', ANIM_ON ? 'true' : 'false');
  document.getElementById('animLabel').textContent =
    'Animation: ' + (ANIM_ON ? 'On' : 'Off');
  // Same predicate as the observer, so a thaw that arrives while the tab is
  // hidden cannot reach a different answer from the callback beside it.
  const may = mayAnimate(true);
  grid.querySelectorAll('video[data-play]').forEach(n => {
    if (!may) {
      setPlaying(n,false);
      if (!ANIM_ON || document.hidden || (ANIM_HOVER && n !== previewVideo)) attachVideo(n,false);
    } else if (animIO) { animIO.unobserve(n); animIO.observe(n); }  // re-evaluate
    else setPlaying(n, true);   // no observer: play what is mounted
  });
  if (!may) allocate();
  else if (animIO) scheduleAllocate();
  else grid.querySelectorAll('img[data-anim]').forEach(showMotion);  // no observer: all
}

function paintAnimAll(){
  document.getElementById('animAll').setAttribute('aria-pressed', ANIM_ALL ? 'true' : 'false');
  document.getElementById('animAllLabel').textContent = 'All visible: ' + (ANIM_ALL ? 'On' : 'Off');
}
function setAnimAll(on){
  ANIM_ALL = !!on;
  prefs.set('animAll', ANIM_ALL ? '1' : '0');
  paintAnimAll();
  scheduleAllocate();
}
document.getElementById('animAll').onclick = () => { remember(); setAnimAll(!ANIM_ALL); };
paintAnimAll();

function paintAnimHover(){
  document.getElementById('animHover').setAttribute('aria-pressed', String(ANIM_HOVER));
  document.getElementById('animHoverLabel').textContent = 'Hover only: ' + (ANIM_HOVER ? 'On' : 'Off');
  /** @type {HTMLButtonElement} */ (document.getElementById('animAll')).disabled = ANIM_HOVER;
}
function setAnimHover(on){
  ANIM_HOVER = !!on;
  prefs.set('animHover', ANIM_HOVER ? '1' : '0');
  paintAnimHover();
  for (const n of grid.querySelectorAll('img[data-anim],video[data-play]')) {
    const thumb = n.closest('.thumb');
    if (ANIM_HOVER) thumb.setAttribute('tabindex', '0'); else thumb.removeAttribute('tabindex');
  }
  applyAnim();
}
document.getElementById('animHover').onclick = () => { remember(); setAnimHover(!ANIM_HOVER); };
paintAnimHover();

// "Real video": the original files in <video> players instead of the light
// animated previews. Changing it rebuilds the mounted video cards, because the
// element type itself changes -- but never under a drag, which holds references
// to the cards it is carrying; that rebuild waits for the drag to end.
let VIDEO_REAL = prefs.get('videoReal', '0') === '1';
let videoRebuildPending = false;
function paintVideoReal(){
  document.getElementById('videoReal').setAttribute('aria-pressed', VIDEO_REAL ? 'true' : 'false');
  document.getElementById('videoRealLabel').textContent = 'Real video: ' + (VIDEO_REAL ? 'On' : 'Off');
}
function rebuildVideoCards(){
  if (dragKey !== null) { videoRebuildPending = true; return; }
  videoRebuildPending = false;
  for (const c of [...cards.values()]) if (c.classList.contains('fmt-video')) unmountCard(c);
  relayout();
}
function setVideoReal(on){
  const changed = VIDEO_REAL !== !!on;
  VIDEO_REAL = !!on;
  prefs.set('videoReal', VIDEO_REAL ? '1' : '0');
  paintVideoReal();
  if (changed) rebuildVideoCards();
}
document.getElementById('videoReal').onclick = () => { remember(); setVideoReal(!VIDEO_REAL); };
paintVideoReal();
