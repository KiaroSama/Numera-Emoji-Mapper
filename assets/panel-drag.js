// panel-drag.js -- drag & drop reordering and edge auto-scroll while dragging.
// Loaded after panel-save.js (a drop queues the order through saveOrder).

// --- Drag & drop reordering (sets the publish order) --------------------
// The MODEL is edited as you drag: every dragover that changes the target
// moves the carried items inside ITEMS and re-projects the grid, so what you
// see IS where they land. The drop only records the result; a cancel puts the
// order taken at dragstart back. There is no index arithmetic at drop time --
// the old scheme measured a `to` before the removal and landed one slot off
// in one direction -- and nothing for a preview and a commit to disagree
// about, because the grid is never anything but ITEMS drawn.
let dragKey = null;
let dragSnap = null;          // the order at dragstart: history on commit, restore on cancel
// The pack the pointer is AIMING at -- never the one the card comes to rest
// above. Those two differ at every pack boundary: dropping on pack 4's FIRST
// card puts the emoji above it, so reading the list afterwards always answers
// "pack 3", which is where a held emoji actually landed (measured). A
// grid-to-grid drag was worse: it never re-stamped at all, so a card moved into
// pack 4 kept pack 1 and cut pack 4 in two.
// `undefined` means no card was ever under the pointer, and then nothing is
// stamped: a destination nobody aimed at is not a destination.
let aimedPack;
/** Record the run under the pointer.
 *
 *  `j + 1` because `runPackAt` scans backwards from `index - 1`: this makes the
 *  AIMED card itself the first candidate, and falls through to its neighbours
 *  only when it carries no pack number of its own. A named function rather than
 *  an inline assignment because a test has no pointer -- it calls this and
 *  exercises the real arithmetic instead of copying it. */
function aimAt(j){ aimedPack = runPackAt(j + 1, carried); }

grid.addEventListener('dragstart',e=>{
  const card=e.target.closest('.card'); if(!card){e.preventDefault();return;}
  const it = ITEMS.find(x=>x.key===card.dataset.key);
  if(!it || it.isLogo){ e.preventDefault(); return; }   // logo is fixed first
  dragKey=card.dataset.key;
  dragSnap=snapshot();
  aimedPack=undefined;              // nothing aimed at yet in THIS drag
  // Dragging a PICKED card carries the whole set; dragging an unpicked one is
  // the single-card gesture that has always been here, untouched.
  carried.clear();
  const keys = (selMode && picked.has(dragKey))
    ? ITEMS.filter(x=>picked.has(x.key) && !x.isLogo).map(x=>x.key)
    : [dragKey];
  for(const k of keys){ carried.add(k); const n = cards.get(k); if(n) n.classList.add('drag'); }
  e.dataTransfer.effectAllowed='move';
  try{e.dataTransfer.setData('text/plain',dragKey);}catch(_){}
});

grid.addEventListener('dragover',e=>{
  if(dragKey===null) return;
  e.preventDefault(); e.dataTransfer.dropEffect='move';
  const card=e.target.closest('.card');
  if(!card || carried.has(card.dataset.key)) return;   // over a card being carried
  const j = ITEMS.findIndex(x=>x.key===card.dataset.key);
  if(j < 0 || ITEMS[j].isLogo) return;      // never ahead of the brand logo
  // Which SIDE of the tile the pointer is on decides before-or-after, so the
  // last slot of a row is reachable and the gesture reads the way it looks.
  const r = card.getBoundingClientRect();
  // The pack comes from the card the pointer is ON, the position from which
  // half of it. Read it BEFORE the move, while the list still says where this
  // card sits.
  aimAt(j);
  moveCarried((e.clientX > r.left + r.width/2) ? j + 1 : j);
});

/** Put the carried run in front of the item now at `slot`, keeping its own
 *  internal order however far it travels. No-op when it is already there. */
function moveCarried(slot){
  const moving = [], rest = [];
  let at = slot;
  for(let i = 0; i < ITEMS.length; i++){
    const it = ITEMS[i];
    if(carried.has(it.key)){ moving.push(it); if(i < slot) at--; }
    else rest.push(it);
  }
  const next = rest.slice(0, at).concat(moving, rest.slice(at));
  if(next.every((x,i)=>x===ITEMS[i])) return;
  ITEMS.length = 0; ITEMS.push(...next);
  relayout();
}

grid.addEventListener('drop',e=>{
  if(dragKey===null) return;
  e.preventDefault();
  stopEdgeScroll();
  if(!acceptDrop()){endDrag(false);return;}
  commitDrag();
  endDrag(true);
});
grid.addEventListener('dragend',()=>endDrag(false));

/** Keep what the drag left in ITEMS: record where it started, save. */
function commitDrag(){
  const order = ITEMS.map(x=>x.key);
  // Pack stamps count as a change too. Comparing the order alone meant a drop
  // that carried a card ACROSS a pack boundary without reordering anything left
  // no history entry, so the owner could not undo it.
  const packs = JSON.stringify(ITEMS.filter(x=>x.pack!=null).map(x=>[x.key,x.pack]));
  if(order.every((k,i)=>k===dragSnap.order[i]) &&
     selSig(ITEMS.filter(x=>x.included).map(x=>x.key))===selSig(dragSnap.included) &&
     packs===JSON.stringify(dragSnap.packs)) return;
  remember(dragSnap);
  logUI('reorder',{count:carried.size});
  saveOrder();
}

function endDrag(committed){
  // A cancelled drag has to put the order back: the model moved with the
  // pointer, so leaving it would make the grid disagree with what was saved.
  if(!committed && dragKey !== null){ applySnapshot(dragSnap,false); }
  for(const k of carried){ const n = cards.get(k); if(n) n.classList.remove('drag'); }
  carried.clear(); dragKey=null; dragSnap=null; aimedPack=undefined;
  stopEdgeScroll();
  // Anything still parked is outside the window now that nothing carries it.
  while(park.firstChild) unmountCard(park.firstChild);
}

// --- Auto-scroll while dragging near an edge ----------------------------
// Without this the drag is trapped in the current viewport: with 200 cards
// there is no way to carry #200 up to #10, because the page will not follow
// the pointer. Speed rises the deeper into the edge band you go, so a nudge
// creeps and a hard push travels.
const EDGE_BAND = 100;      // px from the top/bottom edge where scrolling starts
const EDGE_MAX  = 42;       // px per frame at the very edge
let edgeSpeed = 0, edgeFrame = null;

function edgeScroll(y){
  const over = EDGE_BAND - y;                       // >0 once inside the top band
  const under = y - (innerHeight - EDGE_BAND);      // >0 once inside the bottom band
  const depth = over > 0 ? -over : (under > 0 ? under : 0);
  edgeSpeed = Math.max(-EDGE_MAX, Math.min(EDGE_MAX,
                       Math.round(depth / EDGE_BAND * EDGE_MAX)));
  if(edgeSpeed && edgeFrame === null) stepEdge();
}

function stepEdge(){
  edgeFrame = requestAnimationFrame(()=>{
    edgeFrame = null;
    // Guarded on dragKey as well: a drag that ends outside the window never
    // fires drop, and an unguarded loop would scroll the page forever.
    if(dragKey === null || !edgeSpeed) return;
    scrollBy(0, edgeSpeed);
    stepEdge();
  });
}

function stopEdgeScroll(){
  edgeSpeed = 0;
  if(edgeFrame !== null){ cancelAnimationFrame(edgeFrame); edgeFrame = null; }
}

// On the DOCUMENT, not the grid. Grid events bubble here anyway, and at the top
// of the window the pointer is over the sticky header -- where the grid's own
// handler never fires, which is exactly when scrolling up is wanted.
document.addEventListener('dragover',e=>{
  if(dragKey===null) return;
  e.preventDefault();
  edgeScroll(e.clientY);
});
document.addEventListener('dragend',()=>endDrag(false));
// A drop anywhere outside the grid: cancel rather than reorder by guesswork.
document.addEventListener('drop',e=>{
  if(dragKey===null) return;
  e.preventDefault();
  if(!e.target.closest('#grid')) endDrag(false);
});
