// Repeated-run unhold placement matrix and lifecycle scenario runners for panel_curation.spec.js.
import assert from "node:assert/strict";
import { test, synth } from "../browser-fixture.js";

// Exhaust every four-card stamp layout and nonempty held subset. The expected
// round trip is the captured original model, independent of placement helpers.
function unholdRunMatrix(){
  const base=snapshot(), ordinary=ITEMS.filter(item=>!item.isLogo).map(item=>item.key);
  const bad=[];let cases=0, failures=0;
  for(let layout=0;layout<81;layout++){
    let value=layout;
    const packs=base.packs.filter(([key])=>!ordinary.includes(key));
    for(const key of ordinary){
      const stamp=value%3;value=Math.floor(value/3);
      if(stamp)packs.push([key,stamp]);
    }
    for(let subset=1;subset<16;subset++){
      applySnapshot({...base,packs,holds:[],runs:[]},false);
      past=[];future=[];pastTopSig=null;updateHistoryButtons();
      const before=JSON.stringify(snapshot());
      const keys=ordinary.filter((key,index)=>subset&(1<<index));
      holdKeys(new Set(keys));
      const held=JSON.stringify(snapshot());
      const accepted=unholdKeys(keys), after=JSON.stringify(snapshot());
      let reason=!accepted?'refused':after!==before?'round trip':null;
      if(accepted){
        undo();if(JSON.stringify(snapshot())!==held)reason=reason||'Undo';
        redo();if(JSON.stringify(snapshot())!==after)reason=reason||'Redo';
      }
      cases++;
      if(reason){failures++;if(bad.length<10)bad.push({layout,subset,reason});}
    }
  }
  return {cases,failures,bad};
}

// All ordered nonempty return groups, including partial restores in every
// order. The oracle uses the original keys/stamps, never placement helpers.
function unholdPartitionMatrix(){
  const base=snapshot(), ordinary=ITEMS.filter(item=>!item.isLogo).map(item=>item.key);
  const partitions=mask=>{
    if(!mask)return [[]];
    const result=[];
    for(let group=mask;group;group=(group-1)&mask){
      for(const rest of partitions(mask^group))result.push([group,...rest]);
    }
    return result;
  };
  const plans=Array.from({length:16},(_,mask)=>partitions(mask));
  const bad=[], paths={hold:0,bulk:0,initial:0};let cases=0,failures=0,steps=0;
  for(let layout=0;layout<81;layout++){
    let value=layout;
    const packs=base.packs.filter(([key])=>!ordinary.includes(key));
    for(const key of ordinary){
      const stamp=value%3;value=Math.floor(value/3);
      if(stamp)packs.push([key,stamp]);
    }
    for(let subset=1;subset<16;subset++){
      const keys=ordinary.filter((key,index)=>subset&(1<<index));
      for(const entry of Object.keys(paths)){
        applySnapshot({...base,packs,holds:[],runs:[]},false);
        const original=ITEMS.map(item=>({...item}));
        if(entry==='hold')holdKeys(new Set(keys));
        else if(entry==='bulk')setIncludedMany(ITEMS.filter(item=>keys.includes(item.key)).map(item=>[item,false]));
        else {
          // These are the same production capture functions used at startup;
          // a separate real page also verifies the initially held wiring.
          for(const item of ITEMS)if(keys.includes(item.key))item.included=false;
          const initial=holdRunsFor(ITEMS.map(item=>({...item,included:true})));
          for(const item of ITEMS.filter(item=>keys.includes(item.key))){
            holdOrigins.set(item.key,originFor(item));holdRuns.set(item.key,initial.get(item.key));
          }
        }
        const captured=ITEMS.map(item=>({...item}));
        for(const plan of plans[subset]){
          let current=captured.map(item=>({...item})),reason=null;
          const origins=new Map(holdOrigins),runs=new Map(holdRuns);
          const included=new Set(current.filter(item=>item.included).map(item=>item.key));
          for(const group of plan){
            const returning=ordinary.filter((key,index)=>group&(1<<index));
            current=projectUnhold(current,returning,origins,runs);
            for(const key of returning){included.add(key);origins.delete(key);runs.delete(key);}
            const expected=original.filter(item=>included.has(item.key)).map(item=>item.key);
            const actual=current.filter(item=>item.included).map(item=>item.key);
            if(JSON.stringify(actual)!==JSON.stringify(expected))reason=reason||'partial order';
            if(current.some(item=>!original.some(prior=>prior.key===item.key&&prior.pack===item.pack)))reason=reason||'stamps';
            steps++;
          }
          if(JSON.stringify(current.map(item=>item.key))!==JSON.stringify(original.map(item=>item.key)))reason=reason||'final order';
          cases++;paths[entry]++;
          if(reason){failures++;if(bad.length<10)bad.push({layout,subset,entry,plan,reason});}
        }
      }
    }
  }
  return {cases,steps,paths,failures,bad};
}

export async function runUnholdPlacementScenarios(panel, page, items, open) {
  await test.step("a legacy draft preserves the same edited native-drag slot",async()=>{
    const restored=await page.evaluate("snapshot()");
    await page.click("#undo");
    const legacy=await page.evaluate("snapshot()");
    delete legacy.runs;
    assert.equal(await page.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:legacy})),true);
    const imported=await page.evaluate("snapshot()");
    await page.locator("#holdCards .hcard button").click();
    assert.deepEqual(await page.evaluate("ITEMS.map(item=>item.key)"),restored.order);
    assert.deepEqual(await page.locator("#grid .card").nth(1).getAttribute("data-key"),items[1].key);
    const after=await page.evaluate("snapshot()");
    await page.click("#undo");
    assert.deepEqual(await page.evaluate("snapshot()"),imported);
    await page.click("#redo");
    assert.deepEqual(await page.evaluate("snapshot()"),after);
  });

  await test.step("Unhold stays in the later run of a repeated pack", async () => {
    const model=synth(4,{packs:[1,2,1,1]});
    const runPage=await open(panel,model,{clock:true});
    const before=await runPage.evaluate("snapshot()");
    await runPage.click("#selmode");
    await runPage.locator(`#grid .card[data-key="${model[2].key}"] .thumb`).click();
    await runPage.click("#toHold");
    await runPage.locator("#holdCards .hcard button").click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),before.order);
    assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),before.included);
    assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.pack!=null).map(item=>[item.key,item.pack])"),before.packs);
  });

  for(const logo of ["none","global","explicit"]){
    await test.step(`all four-card Hold/Unhold layouts and subsets: logo=${logo}`,async()=>{
      const model=synth(4,{logo:logo!=="none"});
      if(logo==="explicit")model[0].pack=1;
      const matrixPage=await open(panel,model,{clock:true});
      const started=Date.now();
      const result=await matrixPage.evaluate(unholdRunMatrix);
      console.info(`${new Date().toISOString()} [INFO] Unhold run matrix ${JSON.stringify({logo,...result,wall_ms:Date.now()-started})}`);
      assert.equal(result.cases,1215);
      assert.equal(result.failures,0,JSON.stringify(result.bad));
      const partialStarted=Date.now();
      const partial=await matrixPage.evaluate(unholdPartitionMatrix);
      console.info(`${new Date().toISOString()} [INFO] Unhold partition matrix ${JSON.stringify({logo,...partial,wall_ms:Date.now()-partialStarted})}`);
      assert.equal(partial.cases,36207);
      assert.deepEqual(partial.paths,{hold:12069,bulk:12069,initial:12069});
      assert.equal(partial.steps,95013);
      assert.equal(partial.failures,0,JSON.stringify(partial.bad));
    });
  }

  for(const entry of ["bulk","initial"]){
    await test.step(`${entry} held rows retain one coherent placement layout`,async()=>{
      const model=synth(4,{packs:[1,2,undefined,1],excluded:entry==="initial"?[1,2]:[]});
      const runPage=await open(panel,model,{clock:true});
      if(entry==="bulk")await runPage.click("#none");
      const held=await runPage.evaluate("snapshot()");
      await runPage.click("#unholdAll");
      assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
      assert.equal(await runPage.evaluate(key=>assignedPacks().get(key),model[2].key),2);
      const restored=await runPage.evaluate("snapshot()");
      await runPage.click("#undo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),held);
      await runPage.click("#redo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),restored);
    });
  }

  await test.step("partial returns preserve still-held neighbours and history",async()=>{
    const model=synth(4,{packs:[1,1,1,1]});
    const runPage=await open(panel,model,{clock:true});
    await runPage.evaluate(keys=>holdKeys(new Set(keys)),[model[1].key,model[2].key]);
    const held=await runPage.evaluate("snapshot()");
    await runPage.locator(`#holdCards .hcard[data-key="${model[2].key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),[model[0].key,model[2].key,model[3].key]);
    const partial=await runPage.evaluate("snapshot()");
    await runPage.click("#undo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),held);
    await runPage.click("#redo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),partial);
    await runPage.locator(`#holdCards .hcard[data-key="${model[1].key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
  });

  for(const movedPeer of [false,true]){
    await test.step(`original live neighbours survive ${movedPeer?"a moved held peer":"crossed outside boundaries"}`,async()=>{
      const model=synth(movedPeer?6:5,{packs:movedPeer?[2,1,1,1,1,3]:[1,2,1,1,3]});
      const runPage=await open(panel,model,{clock:true});
      const heldKeys=movedPeer?[model[2].key,model[3].key]:[model[2].key];
      await runPage.evaluate(keys=>holdKeys(new Set(keys)),heldKeys);
      assert.equal(await runPage.evaluate(({carrying,target})=>{
        dragKey=carrying[0];dragSnap=snapshot();carried.clear();
        for(const key of carrying)carried.add(key);
        aimAt(ITEMS.findIndex(item=>item.key===target));moveCarried(ITEMS.length);
        const accepted=acceptDrop();if(accepted)commitDrag();endDrag(accepted);return accepted;
      },{carrying:movedPeer?[model[0].key,model[2].key]:[model[1].key],target:model.at(-1).key}),true);
      const before=await runPage.evaluate("snapshot()");
      const key=model[movedPeer?3:2].key;
      await runPage.locator(`#holdCards .hcard[data-key="${key}"] button`).click();
      const expected=(movedPeer?[1,3,4,5,0]:[0,2,3,4,1]).map(index=>model[index].key);
      assert.deepEqual(await runPage.evaluate("ITEMS.filter(item=>item.included).map(item=>item.key)"),expected);
      const restored=await runPage.evaluate("snapshot()");
      await runPage.click("#undo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),before);
      await runPage.click("#redo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),restored);
      if(movedPeer){
        await runPage.locator(`#holdCards .hcard[data-key="${model[2].key}"] button`).click();
        assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),[1,2,3,4,5,0].map(index=>model[index].key));
      }
    });
  }

  for(const links of ["self","cycle"]){
    await test.step(`imported ${links} links fall back without following forever`,async()=>{
      const model=synth(4,{packs:[1,1,1,1]});
      const runPage=await open(panel,model,{clock:true});
      await runPage.evaluate(keys=>holdKeys(new Set(keys)),[model[1].key,model[2].key]);
      const draft=await runPage.evaluate("snapshot()");
      for(const [key,run] of draft.runs){
        const target=links==="self"?key:key===model[1].key?model[2].key:model[1].key;
        run.previous=target;run.next=target;
      }
      assert.equal(await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:draft})),true);
      const imported=await runPage.evaluate("snapshot()");
      await runPage.click("#unholdAll");
      assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
      await runPage.click("#undo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),imported);
      await runPage.click("#redo");
      assert.equal(await runPage.evaluate("heldOrder().length"),0);
    });
  }

  await test.step("a moved held marker retains its run through draft import and Reset",async()=>{
    const model=synth(7,{packs:[1,1,2,1,1,3,3]});
    // Four separate runs must be mounted together for this native drag.
    const runPage=await open(panel,model,{clock:true,viewport:{width:1200,height:1800}});
    const key=model[3].key;
    await runPage.evaluate(key=>holdKeys(new Set([key])),key);
    await runPage.click("#save");
    await runPage.waitForFunction("__sent('/api/save').length===1");
    await runPage.evaluate("__settle(0,200)");
    await runPage.clock.runFor(50);
    const saved=await runPage.evaluate("snapshot()");
    await runPage.click("#selmode");
    await runPage.locator(`#grid .card[data-key="${model[0].key}"] .thumb`).click();
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] img`).click();
    await runPage.locator(`#grid .card[data-key="${model[0].key}"]`)
      .dragTo(runPage.locator(`#grid .card[data-key="${model[5].key}"]`));
    const moved=await runPage.evaluate("snapshot()");
    const expected=moved.order.filter(item=>item!==key);
    expected.splice(expected.indexOf(model[4].key),0,key);
    assert.notDeepEqual(moved.order,saved.order);
    await runPage.click("#resetAll");
    assert.deepEqual(await runPage.evaluate("snapshot()"),saved);
    assert.equal(await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:moved})),true);
    const imported=await runPage.evaluate("snapshot()");
    for(const field of ["order","included","packs","holds","runs"])assert.deepEqual(imported[field],moved[field]);
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),expected,
      "the moved held marker must return before the surviving member of its original run");
    const restored=await runPage.evaluate("snapshot()");
    await runPage.click("#undo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),imported);
    await runPage.click("#redo");
    assert.deepEqual(await runPage.evaluate("snapshot()"),restored);
    await runPage.click("#resetAll");
    assert.deepEqual(await runPage.evaluate("snapshot()"),saved);
    await runPage.locator(`#holdCards .hcard[data-key="${key}"] button`).click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
  });

  for(const missing of ["head","all members"]){
    await test.step(`a draft restores a run with missing ${missing} anchors`,async()=>{
      const original=synth(6,{packs:[1,2,1,1,1,3]});
      const heldIndex=missing==="head"?3:2;
      const absent=missing==="head"?[2]:[3,4];
      const model=original.filter((item,index)=>!absent.includes(index))
        .map(item=>({...item,included:item.key!==original[heldIndex].key}));
      const runPage=await open(panel,model,{clock:true});
      const draft=await runPage.evaluate("snapshot()");
      draft.order=original.map(item=>item.key);
      draft.included=original.filter((item,index)=>index!==heldIndex).map(item=>item.key);
      draft.packs=original.map(item=>[item.key,item.pack]);
      draft.holds=[[original[heldIndex].key,{index:heldIndex,pack:1,slot:heldIndex-2,anchor:original[2].key}]];
      draft.runs=[[original[heldIndex].key,{first:original[2].key,last:original[4].key,
        before:original[1].key,after:original[5].key,slot:heldIndex-2}]];
      await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:draft}));
      const before=await runPage.evaluate("snapshot()");
      await runPage.locator("#holdCards .hcard button").click();
      const expected=missing==="head"?[0,1,4,3,5]:[0,1,2,5];
      assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),expected.map(index=>original[index].key));
      const after=await runPage.evaluate("snapshot()");
      await runPage.click("#undo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),before);
      await runPage.click("#redo");
      assert.deepEqual(await runPage.evaluate("snapshot()"),after);
    });
  }

  await test.step("legacy drafts and bulk inclusion keep run metadata coherent",async()=>{
    const model=synth(4,{logo:true,packs:[1,2,1,1]});
    const runPage=await open(panel,model,{clock:true});
    const before=await runPage.evaluate("snapshot()");
    await runPage.click("#none");
    await runPage.click("#unholdAll");
    assert.deepEqual(await runPage.evaluate("snapshot()"),before);
    await runPage.click("#none");
    await runPage.click("#all");
    assert.deepEqual(await runPage.evaluate("snapshot()"),before);
    const key=model[3].key;
    await runPage.evaluate(key=>holdKeys(new Set([key])),key);
    const legacy=await runPage.evaluate("snapshot()");
    delete legacy.runs;
    await runPage.evaluate(text=>importDraft(text),JSON.stringify({version:1,snapshot:legacy}));
    await runPage.locator("#holdCards .hcard button").click();
    assert.deepEqual(await runPage.evaluate("ITEMS.map(item=>item.key)"),model.map(item=>item.key));
  });
}
