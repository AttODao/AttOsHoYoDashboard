// Read-only native inspection. Does not establish browser accessibility conformance.
const A=storage.attos;
const errors=[],textOverlaps=[],unlinked=[],controls=[],counts={boards:0,text:0,flex:0,instances:0,tokenBound:0,links:0};
function visible(s){for(let n=s;n;n=n.parent){if(n.hidden)return false;}return true;}
for(const [key,root] of Object.entries(A.boards)){
 counts.boards++;
 const all=[root,...penpotUtils.findShapes(()=>true,root)];
 for(const s of all){
  if(!visible(s))continue;
  if(s.type==='text')counts.text++;
  if(s.flex)counts.flex++;
  if(s.isComponentCopyInstance?.())counts.instances++;
  if(Object.keys(s.tokens||{}).length)counts.tokenBound++;
  counts.links+=(s.interactions||[]).length;
  const b=s.type==='text'?s.textBounds:s.bounds;
  if(b&&s.type!=='group'&&(b.x<root.x-2||b.y<root.y-2||b.x+b.width>root.x+root.width+2||b.y+b.height>root.y+root.height+2))errors.push({key,name:s.name,type:s.type,root:root.bounds,bounds:b});
  if((s.name.startsWith('Button / ')||s.name==='UID selector'||s.name.startsWith('Visibility / '))&&['02','03'].some(p=>A.manifest.find(m=>m.key===key)?.page===p)){
   const disabled=s.component?.()?.name==='Disabled';
   controls.push({key,name:s.name,width:s.width,height:s.height,disabled});
   if(!disabled&&!s.interactions.length)unlinked.push({key,name:s.name});
  }
  if(s.children){const ts=s.children.filter(t=>t.type==='text'&&visible(t));for(let i=0;i<ts.length;i++)for(let j=i+1;j<ts.length;j++){const a=ts[i].textBounds,b=ts[j].textBounds;if(Math.min(a.x+a.width,b.x+b.width)-Math.max(a.x,b.x)>2&&Math.min(a.y+a.height,b.y+b.height)-Math.max(a.y,b.y)>2)textOverlaps.push({key,parent:s.name,a:ts[i].name,b:ts[j].name});}}
 }
}
return {counts,containment:errors,overlaps:textOverlaps,unlinked,smallControls:controls.filter(c=>c.width<44||c.height<44),fileErrors:penpot.currentFile.validate(),pages:penpotUtils.getPages().map(p=>({id:p.id,name:p.name,boards:penpotUtils.getPageById(p.id).root.children.length})),components:penpot.library.local.components.length,tokens:penpotUtils.tokenOverview()};
