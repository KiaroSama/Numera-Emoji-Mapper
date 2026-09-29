// panel-actions.js -- selection gestures, toolbar, copy/toast and the boot block.
// Save queues live in panel-save.js, drag in panel-drag.js, history in panel-holding.js.
// Loaded after
// panel-grid.js, which owns ITEMS, the row model and the mounted cards.
'use strict';

let lastIdx = null;

function setAll(fn){ remember();
  setIncludedMany(ITEMS.filter(it=>!it.isLogo).map(it=>[it, fn(it)]));
  // relayout() too, not just the counter: unticking moves the pack splits.
  relayout(); updateCount(); markSelDirty(); }

// Reduced motion is the one case where nothing plays by itself; hover is then
// the only way to see a video move at all, so the old handlers survive for it.
if (RM) {
  grid.addEventListener('mouseover',e=>{
    const box = e.target.closest('.thumb'); if(!box || box.contains(e.relatedTarget)) return;
    const v = box.querySelector('video'); if(v) setPlaying(v, true);
    // A video card is an animated preview image unless Real video is on.
    const img = !v && box.closest('.fmt-video') && box.querySelector('img[data-anim]');
    if(img && mayAnimate(false, true)) showMotion(img);
  });
  grid.addEventListener('mouseout',e=>{
    const box = e.target.closest('.thumb'); if(!box || box.contains(e.relatedTarget)) return;
    const v = box.querySelector('video'); if(v){ setPlaying(v, false); try{v.currentTime=0;}catch(_){} }
    const img = !v && box.closest('.fmt-video') && box.querySelector('img[data-anim]');
    if(img) showStill(img);
  });
}

// --- Selection mode ------------------------------------------------------
// Arranging a 200-card pack one emoji at a time is the slow part, so a run can
// be picked and carried in one gesture. It is a MODE rather than a modifier
// because the two things a card can be in -- shipped, and moving -- are
// different questions, and a chord that answers both is a chord that answers
// the wrong one by accident.
let selMode = false;

function markPicked(key, on){
  on ? picked.add(key) : picked.delete(key);
  const node = cards.get(key);        // unmounted cards pick up the class on mount
  if(node) node.classList.toggle('picked', on);
}
function clearPicked(){ for(const k of [...picked]) markPicked(k, false); paintSelLabel(); }
function paintSelLabel(){
  document.getElementById('selLabel').textContent =
    !selMode ? 'Selection: Off' : picked.size ? 'Selection: ' + picked.size + ' picked'
                                              : 'Selection: On';
}
document.getElementById('selmode').addEventListener('click', ()=>{
  remember();
  selMode = !selMode;
  document.body.classList.toggle('selmode', selMode);
  document.getElementById('selmode').setAttribute('aria-pressed', selMode ? 'true' : 'false');
  // Leaving the mode drops the picks: a hidden selection that still moves cards
  // on the next drag is the worst version of this feature.
  if(!selMode) clearPicked(); else paintSelLabel();
});

// Drag ACROSS the pick boxes to take a run. Pointer events, not HTML5 drag:
// the card's own draggable is what carries the set, and one element cannot do
// both gestures. Starting on the box is what tells them apart.
let paintFrom = null, paintTo = null;
let strokeBase = null;        // what was picked BEFORE this stroke
let strokeSpan = [];          // indices this stroke is currently claiming
let lastPick = null;          // a key survives reorders, unlike a remembered index
grid.addEventListener('pointerdown', e=>{
  if(!selMode) return;
  const box = e.target.closest('.pick'); if(!box) return;
  const card = box.closest('.card');
  const i = ITEMS.findIndex(x=>x.key===card.dataset.key);
  if(i < 0 || ITEMS[i].isLogo) return;
  e.preventDefault();
  remember();
  const anchor = ITEMS.findIndex(x=>x.key===lastPick && x.included);
  paintFrom = e.shiftKey && anchor >= 0 ? anchor : i;
  if(!e.shiftKey || anchor < 0) lastPick = ITEMS[i].key;
  paintTo = !picked.has(ITEMS[i].key);   // the whole stroke does what the first box did
  // The stroke is re-applied from this baseline on every move, so dragging BACK
  // shrinks the run instead of leaving whatever the pointer already passed.
  strokeBase = new Set(picked);
  strokeSpan = [];
  applyStroke(i);
  try{ box.setPointerCapture(e.pointerId); }catch(_){}
});
grid.addEventListener('pointermove', e=>{
  if(paintFrom === null) return;
  const under = document.elementFromPoint(e.clientX, e.clientY);
  const card = under && under.closest && under.closest('.card');
  if(!card) return;
  const j = ITEMS.findIndex(x=>x.key===card.dataset.key);
  if(j < 0) return;
  applyStroke(j);
});

function applyStroke(j){
  const [a,b] = [Math.min(paintFrom,j), Math.max(paintFrom,j)];
  const span = [];
  for(let k=a;k<=b;k++){ if(!ITEMS[k].isLogo && ITEMS[k].included) span.push(k); }
  const now = new Set(span);
  for(const k of strokeSpan){                    // released by dragging back
    if(!now.has(k)) markPicked(ITEMS[k].key, strokeBase.has(ITEMS[k].key));
  }
  for(const k of span) markPicked(ITEMS[k].key, paintTo);
  strokeSpan = span;
  paintSelLabel();
}
for(const ev of ['pointerup','pointercancel'])
  window.addEventListener(ev, ()=>{ paintFrom = null; });

/** Toggle one card, with Shift taking the range from the last pick.
 *
 *  A CLICK, and on the whole card. That is safe next to the card's own drag
 *  because a drag emits dragstart/drop/dragend and no click at all -- measured
 *  for the tray in feature 002, and a grid card is the same kind of draggable
 *  element. The span rule is `applyStroke`'s: the logo and anything that does
 *  not ship are skipped, so the two ways of selecting cannot disagree. */
function pickCardAt(i, shift){
  remember();
  const anchor = ITEMS.findIndex(x=>x.key===lastPick && x.included);
  const on = !picked.has(ITEMS[i].key);
  const [a,b] = shift && anchor >= 0 ? [Math.min(anchor,i), Math.max(anchor,i)] : [i,i];
  for(let k=a;k<=b;k++){ if(!ITEMS[k].isLogo && ITEMS[k].included) markPicked(ITEMS[k].key, on); }
  if(!shift || anchor < 0) lastPick = ITEMS[i].key;
  paintSelLabel();
}

grid.addEventListener('click',e=>{
  // Copying must not also toggle the card: the label sits inside it, so this
  // has to run first and stop there.
  const cp = e.target.closest('.copyable');
  if(cp){ e.stopPropagation(); copyText(cp.dataset.copy); return; }
  const card = e.target.closest('.card'); if(!card) return;
  const i = ITEMS.findIndex(x=>x.key===card.dataset.key);
  if(i < 0 || ITEMS[i].isLogo) return;   // preview-only card: not toggleable
  // In selection mode the grid is for arranging only -- a stray click must not
  // quietly drop an emoji from the pack -- so the click PICKS instead. It used
  // to do nothing at all here, which left the 17px pick box as the only way to
  // select on a 140px card.
  if(selMode){
    // The box owns its own click: its `pointerdown` has already toggled, and
    // acting on both would toggle twice and net to zero.
    if(!e.target.closest('.pick')) pickCardAt(i, e.shiftKey);
    return;
  }
  remember();
  if(e.shiftKey && lastIdx!==null){
    const [a,b]=[Math.min(lastIdx,i),Math.max(lastIdx,i)];
    const val = !ITEMS[i].included;
    for(let k=a;k<=b;k++){ if(ITEMS[k].isLogo) continue; setIncluded(ITEMS[k], val); }
  } else {
    setIncluded(ITEMS[i], !ITEMS[i].included);
  }
  lastIdx=i; relayout(); updateCount(); markSelDirty();
});

// Plain document scrolling, NOT scrollIntoView. scrollIntoView aligns the
// element with the top of the VIEWPORT, which is behind the sticky header, so
// "Top" stopped one header short and hid the Pack 1 marker; the same alignment
// rule cut the bottom short. The document ends where the last row does -- the
// bottom spacer sees to that -- and the header floats above it.
document.getElementById('top').onclick=()=>window.scrollTo({top:0});
document.getElementById('bot').onclick=()=>
  window.scrollTo({top:document.documentElement.scrollHeight});
document.getElementById('all').onclick=()=>selMode ? pickAll() : setAll(()=>true);
document.getElementById('none').onclick=()=>{ if(selMode){remember();clearPicked();}else setAll(()=>false); };
document.getElementById('inv').onclick=()=>selMode ? invertPicked() : setAll(x=>!x.included);
document.getElementById('anim').onclick=()=>{
  remember();
  ANIM_ON = !ANIM_ON;
  prefs.set('animOn', ANIM_ON ? '1' : '0');
  applyAnim();
};
// Preview backdrop switcher: makes black / hollow / faint emoji visible.
const BGS=['checker','light','dark','gray'];
const BGLABEL={checker:'Checker',light:'Light',dark:'Dark',gray:'Gray'};
function applyBg(b){
  // A stored value can be anything -- an older build's name, or a profile that
  // hands back a string nobody here wrote. Unknown means the default, not a
  // backdrop class that matches no rule and a label reading "undefined".
  if(!BGS.includes(b)) b = 'checker';
  BGS.forEach(x=>document.body.classList.remove('bg-'+x));
  document.body.classList.add('bg-'+b);
  document.getElementById('bg').textContent='Backdrop: '+BGLABEL[b];
  prefs.set('emojiBg', b);
}
document.getElementById('bg').onclick=()=>{
  remember();
  const cur=BGS.find(x=>document.body.classList.contains('bg-'+x))||'checker';
  applyBg(BGS[(BGS.indexOf(cur)+1)%BGS.length]);
};
async function copyText(text){
  if(!text) return;
  // The panel is served from a loopback host, which IS a secure context, so
  // the async clipboard API is normally available. It still REJECTS in real
  // situations -- "Document is not focused" is the one this hit in testing --
  // so a rejection falls through to execCommand rather than giving up.
  if(navigator.clipboard && window.isSecureContext){
    try{ await navigator.clipboard.writeText(text); toast('Copied ' + text); return; }
    catch(_){ /* fall through */ }
  }
  let ok = false;
  try{
    const ta = document.createElement('textarea');
    ta.value = text; ta.setAttribute('readonly','');
    ta.style.cssText = 'position:fixed;top:-1000px';
    document.body.appendChild(ta);
    ta.select(); ta.setSelectionRange(0, text.length);
    ok = document.execCommand('copy');
    ta.remove();
  }catch(_){ ok = false; }
  // Never claim a copy that did not happen: the user would paste whatever was
  // on the clipboard before and not know why it was wrong.
  toast(ok ? 'Copied ' + text : 'Could not copy — select and copy manually');
}

function toast(msg){const t=document.getElementById('toast');t.textContent=msg;
  t.classList.add('show');setTimeout(()=>t.classList.remove('show'),2600);}

// --- boot ----------------------------------------------------------------
applyBg(prefs.get('emojiBg', 'checker'));
loadZoom();
measure();
relayout();
updateCount();
markSelDirty();   // the page loads showing exactly what the server has
applyAnim();
observeLayout();  // after everything render() needs has loaded
