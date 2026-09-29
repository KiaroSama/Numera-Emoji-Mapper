// panel-save.js -- save queues and server liveness: the order and selection
// queues, the heartbeat and the unload guard. Loaded after panel-grid.js and
// before panel-drag.js / panel-actions.js, whose boot calls markSelDirty().

// --- Losing the server must never be silent ------------------------------
// An owner spent three hours reordering a pack while this process was already
// dead. The page looked fine, every drag "worked", and nothing reached the
// catalog. A toast was the only signal and it fades in 2.6 seconds.
//
// So: a banner that stays until the problem is actually gone, an unsaved
// change is remembered and flushed when the server comes back, and the browser
// asks before you close the tab on work that never landed.
let TOK = TOKEN;              // reissued per run; a restart invalidates ours
let lastAlert = '';

// Two queues, one for the order and one for the selection, each holding ONE
// outstanding state stamped with the revision that produced it. That stamp is
// the whole point: an acknowledgement may clear only the revision it
// acknowledges. Without it, an old reply spoke for work it had never carried
// -- a success cleared a newer arrangement outright, and a failure put its own
// stale snapshot back over one -- and the tab was then free to close on both.
let pendingOrder = null;      // newest arrangement not yet acknowledged
let orderRev = 0;             // the revision pendingOrder carries
let orderFlight = 0;          // revision in flight; 0 when idle (single flight)
let orderWait = null, orderBackoff = 0;

let pendingSel = null;        // immutable full body of the latest explicit Save
let selRev = 0, selFlight = 0, selWait = null, selBackoff = 0;

const RETRY_MIN = 1000, RETRY_MAX = 30000;

// One deadline per operation, covering the POST, the 403 token refresh and the
// body read. Without it a fetch that never settles left `orderFlight` set for
// the life of the page: `kickOrder` returns early while a flight is claimed, so
// every newer revision queued behind it silently stopped going.
const REQUEST_TIMEOUT = 15000;

// The earliest time each queue may try again. The 5-second heartbeat used to
// call flushOrder directly, which walked straight past the backoff -- eight
// heartbeats produced eight more identical POSTs after the first refusal. One
// scheduling authority means both the timer and the heartbeat ask this.
let orderNextAt = 0, selNextAt = 0;

// A refusal retrying cannot fix. 400 says the panel's catalog and this page
// have drifted apart, 409 that the page is too old to state its scope: both
// need reconciliation, and re-sending the same body just asks again. The work
// STAYS queued -- beforeunload still guards it -- but nothing resubmits it
// until the request itself changes.
const PERMANENT = new Set([400, 409]);
let orderStuck = false, selStuck = false;
let refusedOrder = null, refusedSel = null;
const orderSig = keys => keys.join('\0');

// What the SERVER has confirmed, as a comparable signature. Distinct from
// `pendingSel`, which is only ever "a Save that has not landed yet": edits made
// while a Save is in flight, or with no Save pressed at all, are unsaved work
// that neither of those told anyone about. An acknowledgement may retire the
// snapshot it carried and nothing newer.
const selSig = (keys) => keys.slice().sort().join('\0');
// Scope, exclusions AND intended packs are one Save, never a live-model retry.
//   `excluded` is full-state -- every key it does not name becomes included --
//   so `known` has to say WHAT THIS PAGE CAN SEE, or the server must assume the
//   tab speaks for the whole catalog. A page opened before an ingest, or before
//   the owner deselected something in another tab, would then silently
//   re-include emoji it has never heard of and be told "Saved".
//   `packs` is the pack each emoji is INTENDED to end up in, which is the whole
//   point of the panel: the server holds what is live, so the difference
//   between the two IS the move plan it writes out.
function selectionBody(){
  const real = ITEMS.filter(x=>!x.isLogo);
  return {excluded: real.filter(x=>!x.included).map(x=>x.key),
          known: real.map(x=>x.key),
          packs: real.filter(x=>x.pack!=null).map(x=>[x.key,x.pack])};
}
// The page never gains or loses a key -- a reorder keeps the same set -- so the
// sorted scope is computed once instead of on every click that marks the page dirty.
const KNOWN_SORTED = ITEMS.filter(x=>!x.isLogo).map(x=>x.key).sort();
function selectionSig(body){
  const known = body.known.length === KNOWN_SORTED.length ? KNOWN_SORTED : body.known.slice().sort();
  return JSON.stringify([known, body.excluded.slice().sort(),
    body.packs.map(p=>p.slice()).sort((a,b)=>a[0]<b[0]?-1:a[0]>b[0]?1:0)]);
}
let ackedSel = selectionSig(selectionBody());

function currentExcluded(){
  return ITEMS.filter(x => !x.isLogo && !x.included).map(x => x.key);
}

/** Does the visible selection differ from what the server has acknowledged? */
function selDirty(){ return selectionSig(selectionBody()) !== ackedSel; }

/** Show it on the control you would press to fix it. */
function markSelDirty(){
  const b = document.getElementById('save');
  if(b) b.classList.toggle('dirty', selDirty());
}

function setAlert(html){
  if(html === lastAlert) return;
  lastAlert = html;
  const a = document.getElementById('alert');
  a.innerHTML = html;
  a.classList.toggle('show', !!html);
  measure(); render();        // the banner pushes the grid down
}

function offline(why){
  const safe = document.createElement('span'); safe.textContent = why;
  setAlert('<b>Not saving.</b> ' + safe.innerHTML +
           ' Your work is only in this page — <b>do not close this tab.</b>' +
           ' Eligible saves resume when the panel is reachable. <button id="exportDraft">Export draft</button>');
  document.getElementById('exportDraft').onclick=exportDraft;
}

/** Take the warning down only when there is nothing left to write.
 *
 *  A ping proves the process is alive, never that anything was stored, and a
 *  saved ORDER says nothing about a SELECTION that never landed. Both used to
 *  clear the banner, which is how a refused Save became invisible. */
function clearAlertIfClean(){
  if(pendingOrder === null && pendingSel === null) setAlert('');
}

/**
 * POST with the mutation token, refreshing it once on 403.
 *
 * The token is per run, so a restarted panel rejects ours. Re-reading it from
 * "/" is same-origin, which is exactly the boundary the token protects, so
 * this weakens nothing -- and it is what turns "restart the panel and lose
 * your afternoon" into "restart the panel and it catches up".
 */
async function apiPost(path, body){
  // ONE deadline for the whole operation. The body read is inside it because a
  // response whose stream never finishes wedges the queue exactly as a request
  // that never responds does, and only the caller's flight flag would ever
  // have noticed -- by staying set forever.
  const ac = new AbortController();
  let timer = 0;
  // RACED, not merely aborted. `AbortController` only helps if the transport
  // honours the signal, and the release of the flight flag must not depend on
  // that: a fetch that ignores it -- or resolves a body stream that never
  // ends -- would leave the queue claimed exactly as before. The abort is
  // still fired, so the real request is really cancelled; the rejection is
  // what guarantees the await settles.
  const deadline = new Promise((_, reject) => {
    timer = setTimeout(() => {
      try{ ac.abort(); }catch(_){}
      reject(new Error('deadline'));
    }, REQUEST_TIMEOUT);
  });
  const bounded = (p) => Promise.race([p, deadline]);
  try{
    const send = () => fetch(path, {method:'POST',
      headers:{'Content-Type':'application/json','X-Panel-Token':TOK},
      body:JSON.stringify(body), signal:ac.signal});
    let r = await bounded(send());
    if(r.status === 403){
      // The token refresh is inside the same deadline: it is another network
      // round trip, and one that hangs strands the save just as surely.
      const res = await bounded(fetch('/', {cache:'no-store', signal:ac.signal}));
      const html = await bounded(res.text());
      const m = /const TOKEN = "([^"]+)"/.exec(html);
      if(m){ TOK = m[1]; r = await bounded(send()); }
    }
    let json;
    try{ json = await bounded(r.json()); }
    catch(err){
      // A successful status without a complete acknowledgement proves nothing.
      // Keep a non-success status useful even when its error body is malformed.
      if(r.ok) throw err;
      json = {};
    }
    const record = json !== null && typeof json === 'object' && !Array.isArray(json);
    if(r.ok){
      const count = n=>Number.isSafeInteger(n) && n>=0;
      if(r.status !== 200 || !record || json.ok !== true ||
         (path === '/api/save' && (!count(json.included) || !count(json.excluded))) ||
         (path === '/api/order' && !count(json.count))){
        throw new Error('invalid save acknowledgement');
      }
    }
    return {ok:r.ok, status:r.status, json:record?json:{}};
  } finally {
    clearTimeout(timer);
    deadline.catch(()=>{});     // nothing is listening once we are done
  }
}

/**
 * Send the pending arrangement, once.
 *
 * `order` is always `pendingOrder` -- the debounce and the heartbeat both
 * flush, and passing it explicitly is what lets an older snapshot handed in by
 * mistake be refused rather than written. A second caller while one is in
 * flight is a no-op: the reply re-arms the queue with whatever is newest by
 * then, so two saves can never race for the same catalog rows.
 */
async function flushOrder(order){
  if(orderFlight || orderStuck || Date.now()<orderNextAt || pendingOrder === null || order !== pendingOrder) return false;
  const rev = orderRev;
  orderFlight = rev;
  let r;
  try{
    r = await apiPost('/api/order', {order});
  }catch(_){
    return failOrder('The panel at this address is not responding.', 0, order);
  }
  if(!r.ok){
    return failOrder(PERMANENT.has(r.status)
      ? 'The panel rejected this arrangement — its catalog no longer matches '
        + 'this page. Export the draft before reconciling the catalog.'
      : 'The panel refused the save (HTTP ' + r.status + ').', r.status, order);
  }
  orderFlight = 0; orderBackoff = 0; orderStuck = false;
  logUI('save_succeeded',{revision:rev,count:order.length});
  // Only the acknowledged revision is saved. An arrangement made WHILE this was
  // in flight is still at risk and goes next, instead of being forgotten the
  // moment an older save came back ✓.
  if(rev !== orderRev){ kickOrder(0); return false; }
  pendingOrder = null;
  clearAlertIfClean();
  return true;
}

function failOrder(why, status, submitted){
  logUI('save_failed',{status,revision:orderFlight});
  orderFlight = 0;
  // The failed snapshot is deliberately NOT written back to pendingOrder: that
  // already holds the newest arrangement, which is this one or something later,
  // and restoring the old one is how a retry overwrote work that came after it.
  offline(why);
  if(PERMANENT.has(status)){
    // Its own comment already said retrying cannot fix a 400 -- and then it
    // scheduled a retry anyway, so a refusal repeated forever. The work stays
    // queued and guarded; what stops is the resubmitting.
    refusedOrder = orderSig(submitted);
    orderStuck = pendingOrder !== null && orderSig(pendingOrder)===refusedOrder;
    clearTimeout(orderWait);
    if(!orderStuck){orderBackoff=0;kickOrder(0);}
    return false;
  }
  orderStuck = false;
  orderBackoff = Math.min(RETRY_MAX, orderBackoff ? orderBackoff * 2 : RETRY_MIN);
  kickOrder(orderBackoff);
  return false;
}

function kickOrder(ms){
  clearTimeout(orderWait);
  if(pendingOrder === null || orderFlight || orderStuck) return;
  orderNextAt = Date.now() + ms;       // the heartbeat reads this too
  orderWait = setTimeout(()=>flushOrder(pendingOrder), ms);
}

function saveOrder(){
  pendingOrder = ITEMS.filter(x=>!x.isLogo).map(x=>x.key);
  orderRev++;                           // at risk from this moment on
  orderStuck = orderSig(pendingOrder)===refusedOrder;
  // The debounce uses the same eligibility gate as retry and heartbeat.
  if(!orderStuck){orderBackoff=0;kickOrder(400);}
}

// Poll for the server rather than waiting for the next drag to discover it is
// gone -- the whole point is to find out while you can still act.
setInterval(async ()=>{
  try{
    const r = await fetch('/api/ping', {cache:'no-store'});
    if(!r.ok) throw new Error(r.status);
    // Through the SAME schedule the timers use. Calling flushOrder directly
    // from here walked past the backoff entirely, so a refusal that had earned
    // a 30-second wait was re-sent every five seconds instead.
    const now = Date.now();
    if(pendingOrder !== null && !orderStuck && !orderFlight && now >= orderNextAt){
      await flushOrder(pendingOrder);
    }
    if(pendingSel !== null && !selStuck && !selFlight && now >= selNextAt){
      await flushSel(pendingSel);
    }
    clearAlertIfClean();
  }catch(_){
    offline('The panel process is not running.');
  }
}, 5000);

// Last line of defence: the browser asks before the tab takes the work with it.
// Either queue counts -- a Save that never landed loses exactly as much as an
// arrangement that never landed, and only the arrangement used to be asked
// about.
addEventListener('beforeunload', e=>{
  if(pendingOrder) return blockUnload(e);
  if(pendingSel) return blockUnload(e);
  // And ordinary unsaved ticks, which neither queue knows about. `pendingSel`
  // means "a Save is still trying"; it says nothing about edits made while one
  // was in flight, or about edits where Save was never pressed. Both are work
  // this tab would take with it.
  if(selDirty()) blockUnload(e);
});
function blockUnload(e){ e.preventDefault(); e.returnValue = ''; }

// Selection persists only when Save is pressed. That is the contract, and a
// retry must not quietly widen it: a refused Save keeps the snapshot it was
// GIVEN and retries exactly that, so ticks made afterwards stay unsaved until
// the owner presses Save again -- the same as if the failure had never
// happened. What did change is that the refusal is no longer forgotten.
async function flushSel(submitted){
  if(selFlight || selStuck || Date.now()<selNextAt || pendingSel === null || submitted !== pendingSel) return false;
  const rev = selRev;
  const checkpoint = pendingSaveSnapshot;
  selFlight = rev;
  let r;
  // Nothing is re-read from ITEMS here: a retry is the same explicitly saved
  // scope, selection and pack intent, even after newer unsaved edits.
  try{ r = await apiPost('/api/save', submitted); }
  catch(_){ return failSel('The panel did not provide a valid save acknowledgement.', 0, submitted); }
  const j = r.json;
  if(!r.ok){
    return failSel('The panel refused the save (' + (j.error || r.status) + ').',
                   r.status, submitted);
  }
  selFlight = 0; selBackoff = 0; selStuck = false;
  ackedSel = selectionSig(submitted);
  logUI('save_succeeded',{revision:rev,count:submitted.excluded.length});
  if(checkpoint) savedSnapshot = checkpoint;
  markSelDirty();
  if(rev !== selRev){ kickSel(0); return false; }
  pendingSel = null;
  clearAlertIfClean();
  const scope = HIDDEN ? ' in the catalog' : '';
  toast(selDirty() ? 'Saved the submitted version; newer changes are unsaved.'
    : `Saved ✓  ${j.included} included · ${j.excluded} excluded${scope}`);
  return true;
}

function failSel(why, status, submitted){
  logUI('save_failed',{status,revision:selFlight});
  selFlight = 0;
  // Not a toast: a failed save you did not see is how an afternoon of work
  // goes missing. The banner stays up until the save actually lands, and the
  // snapshot stays queued so it still can.
  offline(why);
  if(PERMANENT.has(status)){
    // A 409 means this page is too old to say what it was showing, and a 400
    // that the body itself is wrong. Re-sending the same body gets the same
    // answer; the selection stays queued and guarded until a reconciled tab
    // saves it.
    refusedSel = selectionSig(submitted);
    selStuck = pendingSel !== null && selectionSig(pendingSel)===refusedSel;
    clearTimeout(selWait);
    if(!selStuck){selBackoff=0;kickSel(0);}
    return false;
  }
  selStuck = false;
  selBackoff = Math.min(RETRY_MAX, selBackoff ? selBackoff * 2 : RETRY_MIN);
  kickSel(selBackoff);
  return false;
}

function kickSel(ms){
  clearTimeout(selWait);
  if(pendingSel === null || selFlight || selStuck) return;
  selNextAt = Date.now() + ms;
  selWait = setTimeout(()=>flushSel(pendingSel), ms);
}

function queueSelection(checkpoint){
  pendingSel = selectionBody();
  // Snapshot arrays have no references into the mutable item model. Copy the
  // checkpoint as well so Reset always describes the acknowledged Save.
  pendingSaveSnapshot = JSON.parse(JSON.stringify(checkpoint));
  selRev++;
  logUI('save_requested',{revision:selRev,count:pendingSel.excluded.length});
  selStuck = selectionSig(pendingSel)===refusedSel;
  if(!selStuck){selBackoff=0;selNextAt=Date.now();}
  flushSel(pendingSel);
}
document.getElementById('save').onclick=()=>queueSelection(snapshot());
