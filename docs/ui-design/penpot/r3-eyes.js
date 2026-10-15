// Additive, editable controls for the supplied Japanese/dark R3 native snapshot.
// storage.agdR3EyeTargets: [{page,board,input,expected:{x,y,width,height,name},fieldKey}].
// Set storage.agdR3EyePreflight=true for counts only. Never infer targets from text.
// An interrupted eyeInProgress requires inspection, not a blind retry/reset.
const A=storage.agdR3;
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong Penpot file; no mutation');
if(!A?.pages||!A.components||Object.keys(A.components).length<25)throw Error('Expected at least the existing 25 R3 components');
if(A.inProgress||A.glyphRefresh)throw Error('R3 import or glyph refresh pending; no mutation');
if(A.eyeInProgress)throw Error('Prior eye patch interrupted; inspect before resuming');
const targets=storage.agdR3EyeTargets;
if(!Array.isArray(targets)||!targets.length)throw Error('Supply native R3 eye targets');
const assetPath='AGD R3 Draft',buttonName='Button / パスワードを表示';
const pages={};
for(const label of ['Desktop','Mobile','Components']){
 const matches=penpot.currentFile.pages.filter(p=>p.name===assetPath+' / '+label);
 if(matches.length!==1)throw Error('Expected one owning R3 '+label+' page');
 pages[label]=matches[0];
}
const cp=pages.Components;
if(A.pages.components?.id!==cp.id)throw Error('R3 Components storage ownership mismatch');
const components=penpot.library.local.components;
for(const component of Object.values(A.components)){
 if(!component?.id||!components.some(c=>c.id===component.id)||component.path!==assetPath)throw Error('Existing R3 component missing or moved');
}
// Index identities and ancestry only; do not read input/label characters or links.
const indexes=new Map(),markers=[];
for(const page of [pages.Desktop,pages.Mobile,cp]){
 const index=new Map();
 function walk(shape,parent,ancestors){
  if(index.has(shape.id))throw Error('Duplicate native shape identity');
  index.set(shape.id,{shape,parent,ancestors});
  if(page.id!==cp.id&&(shape.getPluginData('agd-r3-eye-for')||shape.getPluginData('agd-r3-action')?.startsWith('reveal-')))markers.push({page,shape});
  for(const child of shape.children||[])walk(child,shape,[...ancestors,shape]);
 }
 walk(page.root,null,[]);indexes.set(page.id,index);
}
const masters=cp.root.children.filter(s=>s.getPluginData('agd-r3-eye-master')==='static-v1');
const eyeComponents=components.filter(c=>c.path===assetPath&&c.name===buttonName);
if(A.eyeComponent){
 if(!components.some(c=>c.id===A.eyeComponent.id)||A.eyeComponent.path!==assetPath||A.eyeComponent.name!==buttonName||eyeComponents.length!==1||masters.length!==1||masters[0].component?.()?.id!==A.eyeComponent.id)throw Error('Eye master missing or inconsistent; inspect before resuming');
}else if(masters.length||eyeComponents.length)throw Error('Unbound eye master exists; inspect rather than duplicate');
const plans=[],fieldKeys=new Set(),inputIds=new Set();let skipped=0;
for(const target of targets){
 const {page,board,input,expected,fieldKey}=target||{};
 if(typeof fieldKey!=='string'||!fieldKey.trim()||fieldKeys.has(fieldKey)||!input?.id||inputIds.has(input.id))throw Error('Invalid or duplicate eye target/fieldKey');
 fieldKeys.add(fieldKey);inputIds.add(input.id);
 const owner=[pages.Desktop,pages.Mobile].find(p=>p.id===page?.id&&p.name===page.name);
 const index=owner&&indexes.get(owner.id),boardRow=index?.get(board?.id),row=index?.get(input.id);
 if(!owner||!boardRow||boardRow.parent?.id!==owner.root.id||boardRow.shape.type!=='board'||!boardRow.shape.getPluginData('agd-r3-key'))throw Error('Target board is not an owning R3 scene');
 if(!row||!row.ancestors.some(s=>s.id===board.id)||row.shape.type!=='board'||row.parent?.id!==input.parent?.id)throw Error('Input is not within the supplied R3 board');
 if(!expected||![expected.x,expected.y,expected.width,expected.height].every(Number.isFinite)||expected.width<48||expected.height<48||typeof expected.name!=='string'||!['x','y','width','height','name'].every(k=>input[k]===expected[k]&&row.shape[k]===expected[k]))throw Error('Input snapshot changed; recapture before patching');
 const parent=row.parent;
 // The sibling control belongs to the ordinary field board, never a component.
 if(parent.type!=='board'||parent.id===board.id||[...row.ancestors.slice(row.ancestors.findIndex(s=>s.id===board.id))].some(s=>s.component?.()))throw Error('Eye parent is not an ordinary field board');
 const action='reveal-'+fieldKey;
 const existing=markers.filter(({shape:s})=>s.getPluginData('agd-r3-action')===action||s.getPluginData('agd-r3-eye-for')===input.id);
 if(existing.length){
  const match=existing[0];
  if(existing.length!==1||!A.eyeComponent||match.page.id!==owner.id||match.shape.parent?.id!==parent.id||match.shape.getPluginData('agd-r3-action')!==action||match.shape.getPluginData('agd-r3-eye-for')!==input.id)throw Error('Conflicting eye marker; preserve existing content and inspect');
  skipped++;continue;
 }
 plans.push({page:owner,parent,input,fieldKey,x:expected.x+expected.width-48,y:expected.y});
}
const counts={targets:targets.length,add:plans.length,skipped,existingComponents:Object.keys(A.components).length,createMaster:plans.length>0&&!A.eyeComponent};
const status='Static editable eye icon only; no executable form behavior, secret disclosure, or links.';
if(storage.agdR3EyePreflight===true)return {phase:'preflight',...counts,status};
if(!plans.length)return {phase:'unchanged',...counts,status};
// Persist before any native mutation. Errors deliberately leave this pending.
A.eyeInProgress={phase:'master',fieldKey:null,completed:0};
if(!A.eyeComponent){
 await penpot.openPage(cp);
 const y=cp.root.children.reduce((bottom,s)=>Math.max(bottom,s.y+s.height),0)+24;
 const master=penpot.createBoard();A.eyeInProgress.master=master;
 cp.root.appendChild(master);master.name=buttonName;master.clipContent=false;master.fills=[];master.strokes=[];master.resize(48,48);master.x=32;master.y=y;
 master.setPluginData('agd-r3-eye-master','static-v1');
 const svg='<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#F8F7FC" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0"/></svg>';
 const icon=await penpot.createShapeFromSvg(svg);master.appendChild(icon);icon.name='Icon / eye';icon.resize(20,20);icon.x=46;icon.y=y+14;
 A.eyeComponent=penpot.library.local.createComponent([master]);A.eyeComponent.name=buttonName;A.eyeComponent.path=assetPath;
}
for(const plan of plans){
 A.eyeInProgress.phase='control';A.eyeInProgress.fieldKey=plan.fieldKey;
 await penpot.openPage(plan.page);
 const button=A.eyeComponent.instance();A.eyeInProgress.control=button;
 plan.parent.appendChild(button);button.name=buttonName;button.resize(48,48);button.x=plan.x;button.y=plan.y;
 button.setPluginData('agd-r3-action','reveal-'+plan.fieldKey);button.setPluginData('agd-r3-eye-for',plan.input.id);
 A.eyeInProgress.completed++;
}
A.eyeInProgress=null;
return {phase:'added',...counts,status};
