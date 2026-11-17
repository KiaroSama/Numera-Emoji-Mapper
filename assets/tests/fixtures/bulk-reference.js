
window.__refOriginFor = function(it){
  const index=ITEMS.indexOf(it);
  let pack=it.pack ?? null;
  if(pack===null){
    const starts=packStarts().starts;
    const run=starts.filter(s=>s.index<=index).pop();
    pack=run ? (run.pack ?? 'new:'+starts.indexOf(run)) : 'new:0';
  }
  const starts=packStarts().starts;
  const run=starts.filter(s=>s.index<=index).pop();
  const start=run?run.index:0;
  return {index,pack,slot:ITEMS.slice(start,index).filter(x=>x.included||x.isLogo).length,
          anchor:ITEMS[start]?.key};
};
window.__bulkEquivalence = function(cases, seed){
  let a = seed >>> 0;
  const rnd = () => { a = (a + 0x6D2B79F5) >>> 0; let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
  const bad = [];
  for (let c = 0; c < cases; c++) {
    const n = 20 + Math.floor(rnd() * 281), withPacks = rnd() < 0.67;
    const excluded = rnd() * 0.3, model = [];
    if (rnd() < 0.5) model.push({key: '__logo', isLogo: true, included: true, fmt: 'static'});
    for (let i = 0; i < n; i++) {
      const item = {key: 'k' + i, fmt: 'static', included: rnd() >= excluded};
      if (withPacks && rnd() < 0.8) item.pack = 1 + Math.floor(rnd() * 4);
      model.push(item);
    }
    const want = new Map(model.filter(x => !x.isLogo).map(x => [x.key, rnd() < 0.5]));
    const load = () => { ITEMS.length = 0; for (const x of model) ITEMS.push({...x}); };
    const dump = m => JSON.stringify([[...m.entries()].sort(), ITEMS.map(x => x.included)]);

    load(); const ref = new Map();
    for (const it of ITEMS) {
      const on = want.get(it.key);
      if (it.isLogo || it.included === on) continue;
      if (!on) ref.set(it.key, __refOriginFor(it)); else ref.delete(it.key);
      it.included = on;
    }
    const expected = dump(ref);
    load(); holdOrigins.clear();
    setIncludedMany(ITEMS.filter(x => !x.isLogo).map(x => [x, want.get(x.key)]));
    if (dump(holdOrigins) !== expected) bad.push(['sequential', c]);

    load();
    const picks = ITEMS.filter(x => !x.isLogo && x.included && rnd() < 0.3);
    const snap = JSON.stringify([...new Map(picks.map(it => [it.key, __refOriginFor(it)])).entries()].sort());
    if (JSON.stringify([...originsFor(picks).entries()].sort()) !== snap) bad.push(['together', c]);
  }
  return bad;
};
