
// Click either id to copy it. Nothing else on this page reacts to a click:
// it is a record, not an editor.
document.addEventListener('click', e => {
  const el = e.target.closest('.id');
  if (!el || !navigator.clipboard) return;
  const text = el.dataset.id || '';
  if (!text) return;
  navigator.clipboard.writeText(text).then(() => {
    const card = el.closest('.card');
    el.classList.add('hit'); card.classList.add('copied');
    setTimeout(() => { el.classList.remove('hit'); card.classList.remove('copied'); }, 700);
  }, () => {});
});

// --- media gating, ported from the curate panel -------------------------- //
// A roster can hold 200 cards, most of them animated. Decoding all of them at
// once is what made this page heavy, so every animated card starts on its STILL
// and only what is actually on screen is swapped to the moving version. The two
// URIs are both inline, so a swap costs no request.
const RM = matchMedia('(prefers-reduced-motion: reduce)').matches;
let ANIM_ON = false;
try { ANIM_ON = localStorage.getItem('rosterAnim') !== '0'; } catch (_) { ANIM_ON = true; }

// Observes the CARD rather than the media inside it: the card is the element
// carrying content-visibility, so it is the one the browser is already deciding
// about. (An earlier note here blamed content-visibility for the observer never
// firing during testing -- that was wrong. The test browser reported
// visibilityState "hidden" with innerHeight 0, where nothing intersects
// anything and no animation SHOULD start. Same gating as the curate panel.)
const cards = () => document.querySelectorAll('.card');
const mediaIn = (card) => card.querySelector('img[data-anim], video[data-play]');
function play(v, on) {
  try { if (on) { const q = v.play(); if (q) q.catch(() => {}); } else v.pause(); } catch (_) {}
}
function setLive(n, live) {
  if (!n) return;
  if (n.dataset.play) { play(n, live); return; }
  const want = live ? n.dataset.anim : n.dataset.still;
  if (want && n.getAttribute('src') !== want) n.src = want;
}
function freezeAll() { cards().forEach(c => setLive(mediaIn(c), false)); }
const io = window.IntersectionObserver ? new IntersectionObserver(es => {
  for (const e of es) setLive(mediaIn(e.target), e.isIntersecting && ANIM_ON && !RM);
}, { root: null, rootMargin: '0px' }) : null;

function applyAnim() {
  const b = document.getElementById('anim');
  b.setAttribute('aria-pressed', ANIM_ON ? 'true' : 'false');
  document.getElementById('animLabel').textContent = 'Animation: ' + (ANIM_ON ? 'On' : 'Off');
  if (!ANIM_ON || RM) { freezeAll(); return; }
  cards().forEach(c => { if (io) { io.unobserve(c); io.observe(c); } });
}
// Scrolling is the one moment the decoding buys nothing: the frames go past too
// fast to read while the compositor is already busy.
let thaw = null;
addEventListener('scroll', () => {
  if (!ANIM_ON || RM) return;
  if (thaw === null) freezeAll(); else clearTimeout(thaw);
  thaw = setTimeout(() => { thaw = null; applyAnim(); }, 180);
}, { passive: true });
document.addEventListener('visibilitychange',
  () => { if (document.hidden) freezeAll(); else applyAnim(); });

document.getElementById('anim').addEventListener('click', () => {
  ANIM_ON = !ANIM_ON;
  try { localStorage.setItem('rosterAnim', ANIM_ON ? '1' : '0'); } catch (_) {}
  applyAnim();
});

// --- the other view-only controls ---------------------------------------- //
const BGS = ['checker', 'light', 'dark', 'gray'];
const BGLABEL = { checker: 'Checker', light: 'Light', dark: 'Dark', gray: 'Gray' };
function applyBg(b) {
  BGS.forEach(x => document.body.classList.remove('bg-' + x));
  document.body.classList.add('bg-' + b);
  document.getElementById('bg').textContent = 'Backdrop: ' + BGLABEL[b];
  try { localStorage.setItem('rosterBg', b); } catch (_) {}
}
document.getElementById('bg').addEventListener('click', () => {
  const cur = BGS.find(x => document.body.classList.contains('bg-' + x)) || 'checker';
  applyBg(BGS[(BGS.indexOf(cur) + 1) % BGS.length]);
});
// scrollTo, not scrollIntoView: the latter aligns with the viewport top, which
// sits behind the sticky header, so Top stopped one header short.
document.getElementById('top').addEventListener('click',
  () => scrollTo({ top: 0, behavior: 'smooth' }));
document.getElementById('bot').addEventListener('click',
  () => scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' }));

let saved = 'checker';
try { saved = localStorage.getItem('rosterBg') || 'checker'; } catch (_) {}
applyBg(BGS.includes(saved) ? saved : 'checker');
applyAnim();
