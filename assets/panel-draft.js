// panel-draft.js -- a draft of the arrangement, out to a file and back in.
// Loaded last: it only uses snapshot()/applySnapshot()/remember() from
// panel-holding.js and toast() from panel-actions.js, at click time.

// A refused Save tells the owner to "export the draft and reload". Exporting
// alone kept a file nothing could read back, and an afternoon of reordering was
// lost that way once.
function exportDraft(){
  const blob=new Blob([JSON.stringify({version:1,snapshot:snapshot()},null,2)],{type:'application/json'});
  const url=URL.createObjectURL(blob), a=el('a');
  a.href=url;a.download='numera-emoji-mapper-draft.json';a.click();
  setTimeout(()=>URL.revokeObjectURL(url),1000);
}

/** The page's own state with the draft's decisions laid over the keys both
 *  know. The catalog may have moved on since the export: keys the page no
 *  longer holds are dropped, keys the draft never saw keep their current
 *  state at the end. View settings (zoom, backdrop, switches) stay as they are. */
function mergeDraft(snap){
  const page=snapshot(), inPage=new Set(page.order);
  const kept=new Set(snap.order.filter(k=>inPage.has(k)));
  const logos=page.order.filter(k=>ITEMS.find(x=>x.key===k)?.isLogo);
  const order=[...logos,
    ...snap.order.filter(k=>kept.has(k)&&!logos.includes(k)),
    ...page.order.filter(k=>!kept.has(k)&&!logos.includes(k))];
  const from=(k,draftVal,pageVal)=>kept.has(k)?draftVal:pageVal;
  const dInc=new Set(snap.included||[]), pInc=new Set(page.included);
  const included=order.filter(k=>from(k,dInc,pInc).has(k));
  const dPacks=new Map(snap.packs||[]), pPacks=new Map(page.packs);
  const packs=order.filter(k=>from(k,dPacks,pPacks).has(k)).map(k=>[k,from(k,dPacks,pPacks).get(k)]);
  const inc=new Set(included), dHolds=new Map(snap.holds||[]), pHolds=new Map(page.holds);
  const holds=order.filter(k=>!inc.has(k)&&from(k,dHolds,pHolds).has(k))
    .map(k=>[k,from(k,dHolds,pHolds).get(k)]);
  return {merged:{...page,order,included,packs,holds,picked:page.picked.filter(k=>inc.has(k))},
          kept:kept.size, dropped:snap.order.filter(k=>!inPage.has(k)).length,
          added:page.order.filter(k=>!kept.has(k)&&!logos.includes(k)).length};
}

function importDraft(text){
  let doc;
  try{ doc=JSON.parse(text); }catch(_){ toast('Not a draft file'); return false; }
  const snap=doc&&doc.snapshot;
  if(!doc||doc.version!==1||!snap||!Array.isArray(snap.order)||!snap.order.every(k=>typeof k==='string')){
    toast('Not a draft this panel can read'); return false;
  }
  const {merged,kept,dropped,added}=mergeDraft(snap);
  // Never posted by itself: the owner reviews the result and presses Save.
  remember(); applySnapshot(merged,false);
  toast(`Imported: ${kept} kept, ${dropped} not in this catalog, ${added} new since the draft. Review, then Save.`);
  return true;
}

const draftFile=/** @type {HTMLInputElement} */ (document.getElementById('draftFile'));
document.getElementById('importDraft').onclick=()=>draftFile.click();
draftFile.addEventListener('change',()=>{
  const file=draftFile.files&&draftFile.files[0];
  if(!file) return;
  const reader=new FileReader();
  reader.onload=()=>{ importDraft(String(reader.result)); draftFile.value=''; };
  reader.readAsText(file);
});
