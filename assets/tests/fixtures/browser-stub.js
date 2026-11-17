
window.__net = {calls: [], pingOk: true, passthrough: false};
window.__rejections = [];
window.__plays = 0;
addEventListener('unhandledrejection', e => window.__rejections.push(String(e.reason)));
const _play = HTMLMediaElement.prototype.play;
HTMLMediaElement.prototype.play = function(){ window.__plays++; return _play.apply(this, arguments); };
const _fetch = window.fetch.bind(window);
window.fetch = function(u, o){
  const s = String(u);
  if(window.__net.passthrough) return _fetch(u, o);
  if(s.indexOf('/api/ping') >= 0){
    return window.__net.pingOk
      ? Promise.resolve({ok: true, status: 200, json: () => Promise.resolve({ok: true})})
      : Promise.reject(new TypeError('offline'));
  }
  if(s.indexOf('/api/order') >= 0 || s.indexOf('/api/save') >= 0){
    const rec = {path: s, body: JSON.parse(o.body), settled: false};
    rec.p = new Promise((res, rej) => { rec.res = res; rec.rej = rej; });
    window.__net.calls.push(rec);
    return rec.p;
  }
  return _fetch(u, o);
};
window.__sent = (kind) => window.__net.calls
  .map((c, i) => ({i: i, path: c.path, body: c.body, settled: c.settled}))
  .filter(c => !kind || c.path.indexOf(kind) >= 0);
window.__settle = (i, status) => {
  const rec = window.__net.calls[i];
  rec.settled = true;
  if(status === 0) rec.rej(new TypeError('network'));
  else rec.res({ok: status < 300, status: status,
                json: () => Promise.resolve(
                  {ok: status < 300, count: (rec.body.order || []).length,
                   included: 1, excluded: 1, error: 'refused'})});
};
// The exact pair of statements a drop runs: the model moves, then the order is
// queued. Driving them directly is what makes an interleaving deterministic;
// `test_F01_a_real_drag...` proves the same path through the real gesture.
window.__reorder = (from, to) => {
  carried.clear(); carried.add(ITEMS[from].key); moveCarried(to); carried.clear(); saveOrder();
};
window.__unload = () => {
  const e = new Event('beforeunload', {cancelable: true});
  dispatchEvent(e);
  return e.defaultPrevented;
};
window.__topRow = () => {
  let r = rowAt(scrollY + headerH - gridTop);
  while(r < rows.length && rows[r].sep) r++;
  return r < rows.length ? {start: rows[r].start, end: rows[r].end} : null;
};
window.__anchor = () => ({
  idx: firstVisibleIndex(),
  cols: G.cols,
  top: window.__topRow(),
  atEnd: Math.ceil(scrollY + innerHeight) >= document.documentElement.scrollHeight - 1,
});
// A real flick keeps firing scroll events, which keeps the freeze window open;
// one scrollBy closes it in 180 ms, long before an observer round trip.
window.__keepScrolling = (ms) => {
  const end = performance.now() + ms;
  const step = () => { scrollBy(0, 50);
                       if(performance.now() < end) requestAnimationFrame(step); };
  step();
};
window.__animating = () => [...document.querySelectorAll('#grid img[data-anim]')]
  .filter(n => n.getAttribute('src') === n.dataset.anim).length;
window.__mounted = () => document.querySelectorAll('#grid img[data-anim]').length;
// Selection state, for the queue suite: what the page shows, what it has been
// told the server holds, and whether it is advertising the difference.
window.__excluded = () => ITEMS.filter(x => !x.isLogo && !x.included).map(x => x.key);
window.__dirty = () => selDirty();
window.__dirtyShown = () =>
  document.getElementById('save').classList.contains('dirty');
window.__toggle = (i) => { ITEMS[i].included = !ITEMS[i].included;
                           setCard(ITEMS[i]); updateCount(); markSelDirty(); };
window.__save = () => document.getElementById('save').click();
