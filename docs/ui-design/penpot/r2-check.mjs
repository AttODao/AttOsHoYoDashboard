// Narrow fake tests safety/control flow. It does NOT validate Penpot APIs/rendering.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const source=fs.readFileSync(new URL('./r2-import.js',import.meta.url),'utf8');
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
const run=new AsyncFunction('penpot','penpotUtils','storage',source);
const finish=new AsyncFunction('penpot','penpotUtils','storage',fs.readFileSync(new URL('./r2-finish.js',import.meta.url),'utf8'));
const guide=new AsyncFunction('penpot','penpotUtils','storage',fs.readFileSync(new URL('./r2-guide.js',import.meta.url),'utf8'));
const refresh=new AsyncFunction('penpot','penpotUtils','storage',fs.readFileSync(new URL('./r2-refresh.js',import.meta.url),'utf8'));
let seq=0;
class Shape{
 constructor(type){this.id='fake-'+seq++;this.type=type;this.name='';this.children=[];this.interactions=[];this.data={};this.fills=[];this.strokes=[];this.width=this.height=1;this.x=this.y=0;this.layoutChild={};}
 appendChild(s){this.children.push(s);s.parent=this;}
 resize(w,h){this.width=w;this.height=h;}
 setPluginData(k,v){this.data[k]=v;}
 getPluginData(k){return this.data[k];}
 applyToken(t,props){this.tokens??={};for(const p of props){this.tokens[p]=t.name;if(p==='strokeColor'&&!this.strokes.length)this.strokes=[{strokeWidth:1}];}}
 applyTypography(t){this.typography=t;this.fontSize=t.fontSize;}
 addFlexLayout(){return this.flex={};}
 component(){return this._component||this.parent?.component();}
 setParentIndex(i){const list=this.parent.children;list.splice(list.indexOf(this),1);list.splice(i,0,this);}
 addInteraction(trigger,action){this.interactions.push({trigger,action});}
 remove(){if(this.parent)this.parent.children=this.parent.children.filter(s=>s!==this);}
 clone(){const s=new Shape(this.type);for(const k of Object.keys(this))if(!['id','parent','children'].includes(k))s[k]=['typography','_component'].includes(k)?this[k]:structuredClone(this[k]);s.children=this.children.map(c=>{const x=c.clone();x.parent=s;return x;});return s;}
}
function fake(){
 const pages=[],colors=[],typographies=[],components=[];
 const api={currentFile:{id:'129977ce-683b-806a-8008-c9d0bedd3212',pages,validate:()=>[]},fonts:{findByName:()=>({variants:['400','500','600','700'].map(fontWeight=>({fontWeight,fontStyle:'normal'}))})},createPage:()=>{const p={id:'fake-page-'+seq++,root:new Shape('root'),flows:[],createFlow(name,start){this.flows.push({name,start});}};pages.push(p);return p;},openPage:async p=>{api.currentPage=p;},createBoard:()=>new Shape('board'),createRectangle:()=>new Shape('rectangle'),createText:t=>{const s=new Shape('text');s.characters=t;return s;},createShapeFromSvg:async svg=>{const s=new Shape('group');if(svg.includes('linearGradient'))s.fills=[{fillColorGradient:{type:'fake-editable-gradient'}}];else s.children=[new Shape('path')];return s;},library:{local:{tokens:{addSet:()=>({active:true,addToken:t=>t})},createColor:()=>{const c={};colors.push(c);return c;},createTypography:()=>{const t={setFont(){}};typographies.push(t);return t;},createComponent:([main])=>{const c={instance:()=>{const s=main.clone();s._component=c;return s;}};components.push(c);return c;}}}};
 return {api,utils:{getPageByName:n=>pages.find(p=>p.name===n)},storage:{},pages,colors,typographies,components};
}
const directory=process.env.ATTOS_DESIGN_CAPTURE_DIR||'/tmp/agd-r2';
const names=['dashboard','login','setup','settings','admin','unknown','queued','loading','empty','error','storage','permission','passkey-cancel','admin-recovery','admin-deletion','setup-cookie','setup-pending','dashboard-new-uids','login-passkey','menu','dashboard-on','dashboard-starrail','menu-admin','deleted','admin-deleted'];
const scenes=[];for(const width of [1440,390])for(const name of names)scenes.push(JSON.parse(fs.readFileSync(path.join(directory,name+'-'+width+'.json'),'utf8')));
const f=fake();const original=f.api.createPage();original.name='Page 1';const human=new Shape('text');human.characters='Preserve human work';original.root.appendChild(human);
for(const scene of scenes){f.storage.agdR2Scene=scene;const r=await run(f.api,f.utils,f.storage);assert.equal(r.key,scene.key);assert.equal(f.storage.agdR2.inProgress,null);
 const frame=f.api.currentPage.root.children.at(-1);const compare=(s,n)=>{if(n.kind==='board'&&n.paint.strokeWidth===0)assert.equal(s.strokes.length,0,'no synthetic default strokes on transparent groups');for(let i=0;i<(n.children||[]).length;i++)compare(s.children[i],n.children[i]);};scene.children.forEach((n,i)=>compare(frame.children[i],n));}
assert.equal(original.root.children[0],human);assert.equal(human.characters,'Preserve human work');
assert.equal(f.pages.filter(p=>p.name.startsWith('AGD R2 Draft /')).length,3);
const desktop=f.utils.getPageByName('AGD R2 Draft / Desktop'),mobile=f.utils.getPageByName('AGD R2 Draft / Mobile');assert.equal(desktop.root.children.length,25);assert.equal(mobile.root.children.length,25);
for(const p of [desktop,mobile])for(let i=1;i<p.root.children.length;i++)assert.ok(p.root.children[i].y>p.root.children[i-1].y+p.root.children[i-1].height,'boards do not overlap');
assert.ok(f.components.length>4&&f.components.length<100,'shared control assets, not one component per label');
const count=f.pages.reduce((n,p)=>n+p.root.children.length,0);await assert.rejects(()=>run(f.api,f.utils,f.storage),/Existing R2 board/);assert.equal(f.pages.reduce((n,p)=>n+p.root.children.length,0),count);
for(const change of [s=>{s.api.currentFile.id='wrong';},s=>{s.api.fonts.findByName=()=>null;},s=>{s.storage.agdR2Scene.palette.canvas='bad';},s=>{s.storage.agdR2Scene.children[0].width=-1;}]){
 const s=fake();s.storage.agdR2Scene=structuredClone(scenes[0]);change(s);await assert.rejects(()=>run(s.api,s.utils,s.storage));assert.equal(s.pages.length,0);assert.equal(s.colors.length,0);assert.equal(s.typographies.length,0);
}
const interrupted=fake();interrupted.storage.agdR2Scene=structuredClone(scenes[0]);interrupted.api.createShapeFromSvg=async()=>{throw Error('synthetic interruption');};await assert.rejects(()=>run(interrupted.api,interrupted.utils,interrupted.storage),/interruption/);assert.ok(interrupted.storage.agdR2.inProgress);interrupted.storage.agdR2Scene=structuredClone(scenes[1]);await assert.rejects(()=>run(interrupted.api,interrupted.utils,interrupted.storage),/Prior import interrupted/);
const wired=await finish(f.api,f.utils,f.storage);assert.ok(wired.linksAdded>0);assert.equal(desktop.flows.length,4);assert.equal(mobile.flows.length,4);
for(const p of [desktop,mobile])for(const b of p.root.children){const k=b.getPluginData('agd-r2-key');assert.ok(b.children.at(-1).name.startsWith('header /'));if(k.startsWith('setup')){const targets=[];const walk=s=>{targets.push(...s.interactions.map(i=>i.action.destination.getPluginData('agd-r2-key')));s.children.forEach(walk);};walk(b);assert.ok(targets.every(t=>/^(login|setup-cookie|setup-pending|dashboard-new-uids)-/.test(t)),'setup cannot navigate to settings or admin');}}
assert.equal((await finish(f.api,f.utils,f.storage)).linksAdded,0,'do not duplicate human prototype links');
const guideResult=await guide(f.api,f.utils,f.storage);assert.equal(guideResult.counts.swatches,14);assert.equal(guideResult.counts.instances,4);assert.equal(f.utils.getPageByName('AGD R2 Draft / Guide').root.children.length,1);
await assert.rejects(()=>guide(f.api,f.utils,f.storage),/Guide marker exists/,'guide does not overwrite human edits');
f.storage.agdR2RefreshPage=desktop.name;const prepared=await refresh(f.api,f.utils,f.storage);assert.ok(prepared.count>0);const pending=f.storage.agdR2.glyphRefresh;const sizes=pending.rows.map(r=>r.size);assert.ok(pending.rows.every(r=>Number(r.shape.fontSize)===Number(r.size)+1&&r.shape.characters===' '));
f.storage.agdR2RefreshPage=mobile.name;await assert.rejects(()=>refresh(f.api,f.utils,f.storage),/Restore pending page/);f.storage.agdR2RefreshPage=desktop.name;assert.equal((await refresh(f.api,f.utils,f.storage)).phase,'restored');assert.equal(f.storage.agdR2.glyphRefresh,null);assert.deepEqual(pending.rows.map(r=>r.shape.fontSize),sizes);assert.ok(pending.rows.every(r=>r.shape.characters===r.text));assert.equal(human.fontSize,undefined);
console.log('PASS: 50 measured scenes import in the narrow fake; controls share '+f.components.length+' assets; wrong file/font/paint/geometry stop before mutations; human work, duplicate and interruption guards preserved. Native validation still required.');
