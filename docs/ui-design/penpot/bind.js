// Recover plugin storage after reconnect. Does not create or overwrite shapes.
const pages={};for(const key of ['00','01','02','03','04']){const p=penpotUtils.getPages().find(p=>p.name.startsWith('Draft / '+key+' / '));if(!p)throw new Error('Missing draft page '+key);pages[key]=penpotUtils.getPageById(p.id);}
const colors=Object.fromEntries(penpot.library.local.colors.filter(c=>c.path==='AttOs Draft').map(c=>[c.name,c.color]));
const tokens={};for(const t of penpot.library.local.tokens.sets.find(s=>s.name==='attos-orbit').tokens){const k=t.name.split('.').pop();tokens[t.type==='spacing'?'space'+k:t.type==='borderRadius'?'radius'+k:t.type==='fontSizes'?'size'+k:k]=t;}
const typo=Object.fromEntries(penpot.library.local.typographies.filter(t=>t.path==='AttOs Draft').map(t=>[t.name,t]));
const components=Object.fromEntries(penpot.library.local.components.filter(c=>c.path.includes('AttOs Draft')).map(c=>[(c.path==='AttOs Draft'?'':c.path.split(' / ').pop()+'/')+c.name,c]));
const A=storage.attos={pages,colors,tokens,typo,components,boards:{},links:[],manifest:[]};
for(const [page,p] of Object.entries(pages)){for(const b of p.root.children){let key=b.getPluginData('attos-key')||({'A / Orbit Folio / 推奨候補・未承認':'concept-a','B / Arcade Index / 比較案・未承認':'concept-b','Components / 編集用マスター':'components'})[b.name];if(key){A.boards[key]=b;A.manifest.push({key,page,id:b.id,name:b.name,width:b.width,height:b.height});}}}
return {bound:A.manifest.length,pages:Object.keys(pages),components:Object.keys(components).length};
