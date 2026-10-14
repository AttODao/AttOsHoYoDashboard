// Penpot MCP plugin code. Creates additive draft pages; never overwrites existing work.
if (storage.attos) throw new Error('Draft storage already exists; do not rerun setup.');
const prefix = 'Draft / ';
const pageNames = ['00 / Design System', '01 / Concepts', '02 / Desktop', '03 / Mobile', '04 / Components'];
const pages = {};
for (const name of pageNames) {
  const existing = penpotUtils.getPageByName(prefix + name);
  if (existing && existing.root.children.length) throw new Error('Existing draft page has content; stop.');
  const p = existing || penpot.createPage(); p.name = prefix + name; pages[name.slice(0,2)] = p;
}
const colors = {
  canvas:'#0C121D', surface:'#151E2C', elevated:'#202D40', line:'#7389A5', text:'#F0F4FC', muted:'#ABBBD0',
  rail:'#213E82', primary:'#BBD0FF', onPrimary:'#112348', genshin:'#A7E2C3', genshinSurface:'#17342F',
  starrail:'#CBBFFF', starrailSurface:'#29243F', warning:'#F3CC84', warningSurface:'#382E1E',
  error:'#FFB8B5', errorSurface:'#3C252D', success:'#A7E2C3', successSurface:'#19332D',
  alternate:'#251E2C', alternateAccent:'#F2AF82'
};
const set = penpot.library.local.tokens.addSet({name:'attos-orbit'});
const tokens = {};
for (const [name,value] of Object.entries(colors)) {
  tokens[name] = set.addToken({type:'color',name:'attos.color.'+name,value});
  const asset = penpot.library.local.createColor(); asset.name='AttOs Draft/'+name; asset.color=value;
}
for (const n of [4,8,12,16,24,32,48,56]) tokens['space'+n] = set.addToken({type:'spacing',name:'attos.space.'+n,value:String(n)});
for (const n of [4,8,12]) tokens['radius'+n] = set.addToken({type:'borderRadius',name:'attos.radius.'+n,value:String(n)});
for (const n of [14,16,20,24,32,48,64]) tokens['size'+n] = set.addToken({type:'fontSizes',name:'attos.type.size.'+n,value:String(n)});
tokens.bodyFamily=set.addToken({type:'fontFamilies',name:'attos.type.bodyFamily',value:'Noto Sans JP'});
tokens.displayFamily=set.addToken({type:'fontFamilies',name:'attos.type.displayFamily',value:'Barlow'});
if (!set.active) set.toggleActive();
const typo = {};
for (const [name,size,family,weight] of [['Body',16,'Noto Sans JP','400'],['Small',14,'Noto Sans JP','400'],['Label',16,'Noto Sans JP','500'],['Section',20,'Noto Sans JP','600'],['Title',32,'Noto Sans JP','600'],['Display',48,'Barlow','600'],['Number',64,'Barlow','600']]) {
  const a=penpot.library.local.createTypography(); a.name='AttOs Draft/'+name;
  const f=penpot.fonts.findByName(family); a.setFont(f,f.variants.find(v=>v.fontWeight===weight && v.fontStyle==='normal'));
  a.fontSize=String(size); a.lineHeight='1.5'; a.letterSpacing='0'; typo[name]=a;
}
const A = storage.attos = {pages,colors,tokens,typo,components:{},boards:{},links:[],manifest:[]};
A.fill=(s,c)=>{s.fills=[{fillColor:colors[c]||c,fillOpacity:1}];if(tokens[c])s.applyToken(tokens[c],['fill']);};
A.radius=(s,n)=>{s.borderRadius=n;if(tokens['radius'+n])s.applyToken(tokens['radius'+n],['borderRadiusTopLeft','borderRadiusTopRight','borderRadiusBottomLeft','borderRadiusBottomRight']);};
A.board=(p,name,x,y,w,h,c='surface',radius=0)=>{let b=penpot.createBoard();b.name=name;b.resize(w,h);b.x=(p?.x||0)+x;b.y=(p?.y||0)+y;if(p)p.appendChild(b);b.clipContent=false;A.fill(b,c);A.radius(b,radius);return b;};
A.rect=(p,name,x,y,w,h,c,r=0)=>{let s=penpot.createRectangle();s.name=name;s.resize(w,h);s.x=p.x+x;s.y=p.y+y;p.appendChild(s);A.fill(s,c);A.radius(s,r);return s;};
A.text=(p,name,value,x,y,w,style='Body',c='text',h)=>{let s=penpot.createText(value);s.name=name;p.appendChild(s);s.x=p.x+x;s.y=p.y+y;s.applyTypography(typo[style]);s.applyToken(tokens['size'+typo[style].fontSize],['fontSize']);// Font-family token binding is unsupported by the connected plugin; typography assets own family.
s.resize(w,h||Math.ceil(Number(typo[style].fontSize)*1.6));s.growType='auto-height';A.fill(s,c);return s;};
A.flex=(b,dir='column',gap=16,pad=24)=>{let f=b.addFlexLayout();f.dir=dir;f.rowGap=gap;f.columnGap=gap;f.horizontalPadding=pad;f.verticalPadding=pad;f.alignItems='start';f.justifyContent='start';f.horizontalSizing='fix';f.verticalSizing='fix';if(tokens['space'+gap])b.applyToken(tokens['space'+gap],['rowGap','columnGap']);if(tokens['space'+pad])b.applyToken(tokens['space'+pad],['paddingLeft','paddingRight','paddingTop','paddingBottom']);return f;};
A.flowText=(b,name,value,style='Body',c='text',h)=>A.text(b,name,value,0,0,b.width-2*(b.flex?.leftPadding||0),style,c,h);
A.component=(name,board)=>{let c=penpot.library.local.createComponent([board]);c.name=name.split('/').pop();c.path='AttOs Draft'+(name.includes('/')?' / '+name.split('/').slice(0,-1).join(' / '):'');A.components[name]=c;return c;};
A.instance=(p,name,key,x,y,w)=>{let s=A.components[key].instance();s.name=name;p.appendChild(s);s.x=p.x+x;s.y=p.y+y;if(w)s.resize(w,s.height);return s;};
A.button=(p,label,x,y,w=160,kind='Secondary',target)=>{let b=A.instance(p,'Button / '+label,'Button/'+kind,x,y,w);if(kind==='Secondary'){b.strokes=[{strokeColor:colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];b.applyToken(tokens.line,['strokeColor']);}const t=penpotUtils.findShape(s=>s.type==='text',b);t.characters=label;if(t.layoutChild)t.layoutChild.horizontalSizing='fill';if(target)A.links.push({shape:b,target});return b;};
A.field=(p,label,value,x,y,w=320,disabled=false)=>{let b=A.board(p,'Field / '+label,x,y,w,92,'surface');A.text(b,'Label',label,0,0,w,'Label');let i=A.instance(b,'Input','Input/'+(disabled?'Disabled':'Default'),0,36,w);let t=penpotUtils.findShape(s=>s.type==='text',i);t.characters=value;t.layoutChild.horizontalSizing='fill';return b;};
A.notice=(p,title,body,x,y,w,kind='warning')=>{let b=A.board(p,'Notice / '+title,x,y,w,96,kind+'Surface',8);A.flex(b,'column',4,16);A.flowText(b,'Status',title,'Label',kind);A.flowText(b,'Recovery',body,'Small',kind);return b;};
A.row=(p,name,x,y,w,h,c='surface')=>A.board(p,name,x,y,w,h,c,8);
A.artboard=(page,key,name,x,y,w,h,c='canvas')=>{if(A.boards[key])throw new Error('Existing draft board: '+key+'; preserve human edits');let b=A.board(null,name,x,y,w,h,c);A.pages[page].root.appendChild(b);b.setPluginData('attos-key',key);b.showInViewMode=true;b.clipContent=true;A.boards[key]=b;A.manifest.push({key,name,page,id:b.id,width:w,height:h});return b;};
await penpot.openPage(pages['04']);
let compBoard=A.artboard('04','components','Components / 編集用マスター',0,0,1200,1600);
A.text(compBoard,'Title','共通コンポーネント',48,32,1000,'Title');
A.text(compBoard,'Help','Assetsで再利用。文字・色・余白はすべて編集可能。',48,96,1000,'Body','muted');
let y=160;
for (const [kind,bg,fg] of [['Primary','primary','onPrimary'],['Secondary','elevated','text'],['Disabled','elevated','muted'],['Danger','errorSurface','error']]) {
 let b=A.board(compBoard,'Button/'+kind,48,y,288,48,bg,8);A.flex(b,'row',8,12);b.flex.alignItems='center';b.flex.justifyContent='center';let t=A.flowText(b,'Label',kind==='Primary'?'保存する':kind==='Disabled'?'現在は利用できません':kind==='Danger'?'削除する':'状態を更新','Label',fg);t.layoutChild.horizontalSizing='fill';t.align='center';A.component('Button/'+kind,b);y+=80;
}
for (const [kind,fg] of [['Default','text'],['Disabled','muted']]) {
 let b=A.board(compBoard,'Input/'+kind,400,kind==='Default'?160:240,360,48,'canvas',8);b.strokes=[{strokeColor:colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];b.applyToken(tokens.line,['strokeColor']);A.flex(b,'row',8,12);b.flex.alignItems='center';let t=A.flowText(b,'Value','入力内容','Body',fg);t.layoutChild.horizontalSizing='fill';A.component('Input/'+kind,b);
}
for (const [label,kind] of [['確認待ち','warning'],['保存しました','success'],['取得できません','error']]) {let b=A.notice(compBoard,label,'状態と回復方法を同じ場所に表示します。',400,360+(['確認待ち','保存しました','取得できません'].findIndex(x=>x===label))*128,688,kind);A.component('Notice/'+kind,b);}
let badge=A.board(compBoard,'Badge/Game',48,560,180,44,'genshinSurface',8);A.flex(badge,'row',8,8);badge.flex.alignItems='center';let bt=A.flowText(badge,'Label','原神','Label','genshin');bt.align='center';A.component('Badge/Game',badge);
let metric=A.board(compBoard,'Metric',48,672,288,120,'surface',8);A.flex(metric,'column',8,16);A.flowText(metric,'Label','天然樹脂','Body','muted');A.flowText(metric,'Value','142 / 200','Section');A.component('Metric',metric);
let checkbox=A.board(compBoard,'Checkbox',48,840,288,48,'surface',8);A.rect(checkbox,'Box',12,12,24,24,'primary',4);A.rect(checkbox,'Checked indicator',19,19,10,10,'onPrimary',0);A.text(checkbox,'Label','ホームに表示',48,10,224,'Label');A.component('Checkbox/On',checkbox);
let off=checkbox.clone();off.name='Checkbox/Off';off.y+=80;penpotUtils.findShape(s=>s.name==='Checked indicator',off).hidden=true;let offBox=penpotUtils.findShape(s=>s.name==='Box',off);A.fill(offBox,'elevated');offBox.strokes=[{strokeColor:colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];offBox.applyToken(tokens.line,['strokeColor']);A.component('Checkbox/Off',off);
A.text(compBoard,'State guidance','Hover: elevated / Focus: 2px primary・4px offset\nBusy: 操作全体を無効化し「処理中」を表示\nDisabled: 説明を近接表示。色だけで区別しない。',48,1056,1000,'Body','muted',180);
return {pages:Object.entries(pages).map(([key,p])=>({key,id:p.id,name:p.name})),components:Object.keys(A.components),tokens:set.tokens.length};
