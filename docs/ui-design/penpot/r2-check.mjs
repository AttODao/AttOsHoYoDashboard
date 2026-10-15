// Narrow fake tests safety/control flow. It does NOT validate Penpot APIs/rendering.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const source=fs.readFileSync(new URL('./r2-import.js',import.meta.url),'utf8');
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
const run=new AsyncFunction('penpot','penpotUtils','storage',source);
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
const directory=process.env.ATTOS_DESIGN_CAPTURE_DIR||'/tmp/agd-r3';
const manifest=JSON.parse(fs.readFileSync(path.join(directory,'manifest.json'),'utf8'));
const scenes=manifest.keys.map(key=>JSON.parse(fs.readFileSync(path.join(directory,key+'.json'),'utf8')));
assert.equal(scenes.length,43);
const f=fake(),original=f.api.createPage();original.name='Page 1';const human=new Shape('text');human.characters='Preserve human work';original.root.appendChild(human);
// A second revision must not replace even an existing R2 board with the same key.
f.storage.agdR2Scene={...structuredClone(scenes[0]),source:'agd-r2-local-synthetic'};
const old=await run(f.api,f.utils,f.storage),oldPage=f.api.currentPage,oldBoard=oldPage.root.children[0];
for(const scene of scenes){
 f.storage.agdR2Scene=scene;assert.equal((await run(f.api,f.utils,f.storage)).key,scene.key);assert.equal(f.storage.agdR3.inProgress,null);
 const frame=f.api.currentPage.root.children.at(-1);
 const compare=(s,n)=>{if(n.kind==='board'&&n.paint.strokeWidth===0)assert.equal(s.strokes.length,0);if(n.kind==='text')assert.equal(s.lineHeight,String(n.type.lineHeight));for(let i=0;i<(n.children||[]).length;i++)compare(s.children[i],n.children[i]);};
 scene.children.forEach((n,i)=>compare(frame.children[i],n));
}
assert.equal(original.root.children[0],human);assert.equal(human.characters,'Preserve human work');assert.equal(oldPage.root.children[0],oldBoard);assert.equal(oldBoard.id,old.id);
assert.notEqual(f.storage.agdR2,f.storage.agdR3);
const desktop=f.utils.getPageByName('AGD R3 Draft / Desktop'),mobile=f.utils.getPageByName('AGD R3 Draft / Mobile');assert.equal(desktop.root.children.length,21);assert.equal(mobile.root.children.length,22);
for(const p of [desktop,mobile])for(let i=1;i<p.root.children.length;i++)assert.ok(p.root.children[i].y>p.root.children[i-1].y+p.root.children[i-1].height);
assert.ok(Object.keys(f.storage.agdR3.components).length>4&&Object.keys(f.storage.agdR3.components).length<50);
const count=f.pages.reduce((n,p)=>n+p.root.children.length,0);await assert.rejects(()=>run(f.api,f.utils,f.storage),/Existing R3 board/);assert.equal(f.pages.reduce((n,p)=>n+p.root.children.length,0),count);
for(const change of [s=>{s.api.currentFile.id='wrong';},s=>{s.api.fonts.findByName=()=>null;},s=>{s.storage.agdR2Scene.palette.canvas='bad';},s=>{s.storage.agdR2Scene.children[0].width=-1;},s=>{s.storage.agdR2Scene.source='other';}]){
 const s=fake();s.storage.agdR2Scene=structuredClone(scenes[0]);change(s);await assert.rejects(()=>run(s.api,s.utils,s.storage));assert.equal(s.pages.length,0);assert.equal(s.colors.length,0);
}
const interrupted=fake();interrupted.storage.agdR2Scene=structuredClone(scenes.find(s=>s.key==='dashboard-1440'));interrupted.api.createShapeFromSvg=async()=>{throw Error('synthetic interruption');};await assert.rejects(()=>run(interrupted.api,interrupted.utils,interrupted.storage),/interruption/);assert.ok(interrupted.storage.agdR3.inProgress);interrupted.storage.agdR2Scene=structuredClone(scenes[1]);await assert.rejects(()=>run(interrupted.api,interrupted.utils,interrupted.storage),/Prior import interrupted/);
const r3finish=new AsyncFunction('penpot','penpotUtils','storage',fs.readFileSync(new URL('./r3-finish.js',import.meta.url),'utf8'));
assert.equal((await r3finish(f.api,f.utils,f.storage)).linkcount,171);assert.equal((await r3finish(f.api,f.utils,f.storage)).linkcount,0);
for(const p of [desktop,mobile]){
 assert.equal(p.flows.length,5);
 for(const b of p.root.children){const key=b.getPluginData('agd-r3-key');if(!/^(setup|recovery)-/.test(key))continue;const targets=[];const walk=s=>{targets.push(...s.interactions.map(i=>i.action.destination.getPluginData('agd-r3-key')));s.children.forEach(walk);};walk(b);
 assert.ok(targets.every(t=>/^(login|setup-cookie|setup-pending|dashboard)-/.test(t)));if(key.startsWith('recovery'))assert.ok(targets.every(t=>!/setup-cookie|setup-pending/.test(t)));
 }
}
f.storage.agdR2RefreshPage=desktop.name;const prepared=await refresh(f.api,f.utils,f.storage);assert.ok(prepared.count>0);const pending=f.storage.agdR3.glyphRefresh;
assert.ok(pending.rows.every(r=>Number(r.shape.fontSize)===Number(r.size)+1&&r.shape.characters===' '));
f.storage.agdR2RefreshPage=mobile.name;await assert.rejects(()=>refresh(f.api,f.utils,f.storage),/Restore pending page/);
f.storage.agdR2RefreshPage=desktop.name;await refresh(f.api,f.utils,f.storage);assert.equal(f.storage.agdR3.glyphRefresh,null);assert.ok(pending.rows.every(r=>r.shape.characters===r.text&&r.shape.fontSize===r.size));assert.equal(human.fontSize,undefined);
console.log('PASS: 43 R3 scenes / '+Object.keys(f.storage.agdR3.components).length+' shared assets; isolated R2/human work, paint/line-height, preflight/duplicate/interruption guards, 171 idempotent links, recovery without Cookie, and two-phase text restoration. Narrow fake only.');
