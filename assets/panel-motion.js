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

function attachVideo(v, on){
  on=on && ANIM_ON && !document.hidden;
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
 *  `hover` is the reduced-motion exception. Nothing plays by itself there, so
 *  hover is the only way to see a video move at all -- but it is still no way
 *  round the switch, a hidden tab or a scroll. */
function mayAnimate(inView, hover){
  if(!ANIM_ON || document.hidden || scrollThaw !== null) return false;
  return hover ? true : (inView && !RM);
}

// Swapping an <img> src is cheap -- both URLs are immutable-cached, so this
// never refetches -- which is what makes this affordable where
// mounting/destroying a player was not.
const animIO = window.IntersectionObserver ? new IntersectionObserver(es => {
  for (const e of es) {
    const t = e.target;
    const live = mayAnimate(t.isConnected && e.isIntersecting && e.boundingClientRect.bottom>headerH);
    if (t.dataset.play) { setPlaying(t, live); continue; }
    const want = live ? t.dataset.anim : t.dataset.still;
    t.style.willChange=live?'transform':'';
    if (want && t.getAttribute('src') !== want) t.src = want;
  }
}, {root: null, rootMargin: '0px'}) : null;   // see freezeAll(): a band
// beyond the viewport animated a row nobody was looking at, above AND below.

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

function animatedNodes(){
  return grid.querySelectorAll('img[data-anim], video[data-play]');
}

/** Hold every mounted card on frame 0. Costs no request: the still is the
 *  same immutable-cached URL the card was built with. */
function freezeAll(){
  grid.querySelectorAll('video[data-play]').forEach(v => {setPlaying(v,false);if(document.hidden||!ANIM_ON)attachVideo(v,false);});
  grid.querySelectorAll('img[data-anim]').forEach(img => {
    img.style.willChange='';
    if (img.getAttribute('src') !== img.dataset.still) img.src = img.dataset.still;
  });
}

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
addEventListener('scroll', ()=>{
  scheduleRender();
  // Not `|| RM`: under reduced motion a hovered video is the one thing that
  // can be playing, and `scrollThaw` is how mayAnimate() knows to hold it.
  if(!ANIM_ON) return;
  if(scrollThaw === null) freezeAll();
  else clearTimeout(scrollThaw);
  scrollThaw = setTimeout(()=>{ scrollThaw = null; applyAnim(); }, 180);
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
  animatedNodes().forEach(n => {
    if (!may) {
      if (n.dataset.play){setPlaying(n,false);if(!ANIM_ON||document.hidden)attachVideo(n,false);}
      else {n.style.willChange='';if(n.getAttribute('src')!==n.dataset.still)n.src=n.dataset.still;}
    } else if (animIO) { animIO.unobserve(n); animIO.observe(n); }  // re-evaluate
    else if (n.dataset.play) setPlaying(n, true);   // no observer: play what is mounted
  });
}
