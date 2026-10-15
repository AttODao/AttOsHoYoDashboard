// Run twice for one R2/R3 page: prepare on the first RPC, restore on the next.
// The UI must render between calls. Clear copied content to a space (empty is
// rejected) and restore it next time; same-call updates can leave master glyphs.
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong file');
const page=penpotUtils.getPageByName(storage.agdR2RefreshPage),match=page?.name.match(/^AGD (R2|R3) Draft \/ (Desktop|Mobile|Guide)$/);
if(!match)throw Error('Choose an R2/R3 scene or Guide page');
const A=storage['agd'+match[1]],prefix='agd-'+match[1].toLowerCase();if(!A||A.inProgress)throw Error('Missing assets or interrupted import');
if(A.glyphRefresh&&A.glyphRefresh.page.id!==page.id)throw Error('Restore pending page first');
await penpot.openPage(page);
if(A.glyphRefresh){
 const saved=A.glyphRefresh;
 for(const {shape,size,text}of saved.rows){shape.characters=text;shape.fontSize=size;if(A.sizes?.[Number(size)])shape.applyToken(A.sizes[Number(size)],['fontSize']);}
 A.glyphRefresh=null;
 return {page:page.name,phase:'restored',count:saved.rows.length};
}
const rows=[];
function collect(s){
 if(s.type==='text'&&s.component?.()&&s.characters){const size=s.fontSize;if(!Number.isFinite(Number(size))||Number(size)<=0)throw Error('Invalid copied text size');rows.push({shape:s,size,text:s.characters});}
 for(const child of s.children||[])collect(child);
}
for(const board of page.root.children)if(board.getPluginData(prefix+'-key')||board.getPluginData(prefix+'-guide')==='complete-v1')collect(board);
// Save originals before the first mutation; interruption must be restored, not rerun.
A.glyphRefresh={page,rows};
for(const {shape,size}of rows){shape.characters=' ';shape.fontSize=String(Number(size)+1);}
return {page:page.name,phase:'prepared — run again to restore',count:rows.length};
