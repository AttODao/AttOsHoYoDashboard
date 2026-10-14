// PREPARED OFFLINE, not executed in Penpot. One synthetic scene per call.
// Set storage.agdR2Scene to a JSON emitted by preview/check.mjs, then run this file.
// Native API/gradient/font rendering must be verified before bulk use.
const scene=storage.agdR2Scene;
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong Penpot file; no mutation');
if(!scene||!/^[a-f0-9]{64}$/.test(scene.sourceHash)||scene.schema!==1||scene.source!=='agd-r2-local-synthetic'||!/^[-a-z0-9]+-(390|1440)$/.test(scene.key))throw Error('Invalid synthetic scene');
const pageName='AGD R2 Draft / '+(scene.width===390?'Mobile':'Desktop');
const existing=penpotUtils.getPageByName(pageName);
if(existing?.root.children.some(s=>s.getPluginData('agd-r2-key')===scene.key))throw Error('Existing R2 board; preserve human edits');
if(!storage.agdR2&&penpotUtils.getPageByName('AGD R2 Draft / Components'))throw Error('Existing R2 assets; rebind, do not regenerate');
if(storage.agdR2?.inProgress)throw Error('Prior import interrupted; inspect before resuming');
const validColor=p=>p&&/^#[0-9A-F]{6}$/.test(p.color)&&Number.isFinite(p.opacity)&&p.opacity>=0&&p.opacity<=1;
if(!scene.palette||!Object.values(scene.palette).every(v=>/^#[0-9A-F]{6}$/.test(v)))throw Error('Invalid palette');
const nodes=[];const walk=(n,depth=0)=>{if(depth>80||nodes.length>=2000||!Array.isArray(n.children||[]))throw Error('Invalid scene tree');nodes.push(n);for(const c of n.children||[])walk(c,depth+1);};scene.children.forEach(n=>walk(n));
if(![390,1440].includes(scene.width)||!scene.key.endsWith('-'+scene.width)||!Number.isFinite(scene.height)||scene.height<=0||scene.height>10000)throw Error('Invalid scene dimensions');
const fonts=new Map();
for(const n of nodes){
 if(!['board','text','icon','check'].includes(n.kind)||![n.x,n.y,n.width,n.height].every(Number.isFinite)||n.width<=0||n.height<=0)throw Error('Invalid node');
 if(n.kind==='board'){
  if(!validColor(n.paint?.fill)||!validColor(n.paint?.stroke)||!Number.isFinite(n.paint.radius)||n.paint.radius<0||!Number.isFinite(n.paint.strokeWidth)||n.paint.strokeWidth<0)throw Error('Invalid native paint');
  if(n.paint.gradient&&(!Number.isFinite(n.paint.gradient.angle)||n.paint.gradient.stops.length!==2||!n.paint.gradient.stops.every(validColor)))throw Error('Invalid gradient');
 }
 if(n.kind==='icon'&&(!Array.isArray(n.paths)||!n.paths.length||n.paths.length>8||!validColor(n.color)||!Number.isFinite(n.strokeWidth)))throw Error('Invalid icon');
 if(n.kind==='text'){
  if(typeof n.text!=='string'||!['Noto Sans JP','Orbitron'].includes(n.type.family)||!Number.isFinite(n.type.size)||n.type.size<12||n.type.size>80||!Number.isFinite(n.type.lineHeight)||!validColor(n.type.color))throw Error('Invalid typography');
  const k=[n.type.family,n.type.size,n.type.weight,n.type.lineHeight].join('/');
  if(!fonts.has(k)){const font=penpot.fonts.findByName(n.type.family);const variant=font?.variants.find(v=>String(v.fontWeight)===String(n.type.weight)&&v.fontStyle==='normal');if(!variant)throw Error('Required font variant unavailable: '+k);fonts.set(k,{font,variant,type:n.type});}
 }
}
const A=storage.agdR2||(storage.agdR2={pages:{},typo:{},components:{},tokens:{}});A.inProgress=scene.key;
if(!A.set){
 A.set=penpot.library.local.tokens.addSet({name:'agd-r2-draft'});
 for(const [name,value] of Object.entries(scene.palette)){
  const asset=penpot.library.local.createColor();asset.name='AGD R2/'+name;asset.color=value;
  A.tokens[value]=A.set.addToken({type:'color',name:'agd.r2.color.'+name,value});
 }
 if(!A.set.active)A.set.toggleActive();
}
A.sizes??={};A.radii??={};A.spaces??={};
const dimension=(map,type,name,value)=>map[value]||(map[value]=A.set.addToken({type,name:'agd.r2.'+name+'.s'+String(value).replace('.','_'),value:String(value)}));
for(const f of fonts.values())dimension(A.sizes,'fontSizes','type.size',f.type.size);
for(const n of nodes)if(n.paint?.radius)dimension(A.radii,'borderRadius','radius',n.paint.radius);
for(const [k,f]of fonts)if(!A.typo[k]){const t=penpot.library.local.createTypography();t.name='AGD R2/'+k;t.setFont(f.font,f.variant);t.fontSize=String(f.type.size);t.lineHeight=String(f.type.lineHeight);t.letterSpacing='0';A.typo[k]=t;}
const fill=(s,p)=>{s.fills=p.opacity?[{fillColor:p.color,fillOpacity:p.opacity}]:[];if(p.opacity===1&&A.tokens[p.color])s.applyToken(A.tokens[p.color],['fill']);};
const position=(s,n,dx,dy)=>{s.resize(n.width,n.height);s.x=n.x+dx;s.y=n.y+dy;};
const escape=s=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&apos;'}[c]));
async function style(s,n){
 fill(s,n.paint.fill);s.borderRadius=n.paint.radius;if(n.paint.radius)s.applyToken(A.radii[n.paint.radius],['borderRadiusTopLeft','borderRadiusTopRight','borderRadiusBottomLeft','borderRadiusBottomRight']);
 s.strokes=n.paint.strokeWidth&&n.paint.stroke.opacity?[{strokeColor:n.paint.stroke.color,strokeWidth:n.paint.strokeWidth,strokeStyle:'solid',strokeAlignment:'inner'}]:[];
 if(n.paint.stroke.opacity===1&&A.tokens[n.paint.stroke.color])s.applyToken(A.tokens[n.paint.stroke.color],['strokeColor']);
 if(n.paint.gradient){
  const g=n.paint.gradient,a=g.angle*Math.PI/180,ux=Math.sin(a),uy=-Math.cos(a),len=Math.abs(n.width*ux)+Math.abs(n.height*uy);
  const svg=`<svg xmlns="http://www.w3.org/2000/svg" width="${n.width}" height="${n.height}"><defs><linearGradient id="g" x1="${.5-len*ux/2/n.width}" y1="${.5-len*uy/2/n.height}" x2="${.5+len*ux/2/n.width}" y2="${.5+len*uy/2/n.height}">${g.stops.map((p,i)=>`<stop offset="${i}" stop-color="${p.color}" stop-opacity="${p.opacity}"/>`).join('')}</linearGradient></defs><rect width="${n.width}" height="${n.height}" fill="url(#g)"/></svg>`;
  const brush=await penpot.createShapeFromSvg(svg);
  const find=s=>s.fills?.some(f=>f.fillColorGradient)?s:(s.children||[]).map(find).find(Boolean);
  const gradient=find(brush);if(!gradient){brush.remove();throw Error('SVG gradient did not become an editable native fill; stop');}
  s.fills=gradient.fills;brush.remove();
 }
}
const skeleton=n=>({kind:n.kind,type:n.type,paths:n.paths,paint:n.paint,inputType:n.inputType,checked:n.checked,children:(n.children||[]).map(skeleton)});
const componentKey=n=>n.component+' / '+JSON.stringify(skeleton(n));
function textStyle(s,n){const k=[n.type.family,n.type.size,n.type.weight,n.type.lineHeight].join('/');s.applyTypography(A.typo[k]);s.applyToken(A.sizes[n.type.size],['fontSize']);s.align=['center','right'].includes(n.type.align)?n.type.align:'left';s.growType='auto-height';fill(s,n.type.color);}
function overrides(s,n,dx,dy){
 s.name=n.name;position(s,n,dx,dy);
 if(n.kind==='text'){s.characters=n.text;s.fontSize=String(n.type.size+1);textStyle(s,n);position(s,n,dx,dy);}
 if(n.action)s.setPluginData('agd-r2-action',n.action);
 // No children are inserted or removed inside a component instance.
 if(n.children){if(s.children.length!==n.children.length)throw Error('Component topology mismatch; stop');n.children.forEach((c,i)=>overrides(s.children[i],c,dx,dy));}
}
async function draw(parent,n,dx,dy,instances=true){
 let s;
 if(instances&&n.component){s=A.components[componentKey(n)].instance();parent.appendChild(s);overrides(s,n,dx,dy);return s;}
 if(n.kind==='text'){s=penpot.createText(n.text);parent.appendChild(s);textStyle(s,n);}
 else if(n.kind==='icon'){
  const svg=`<svg xmlns="http://www.w3.org/2000/svg" width="${n.width}" height="${n.height}" viewBox="0 0 24 24" fill="none" stroke="${n.color.color}" stroke-width="${n.strokeWidth}" stroke-linecap="round" stroke-linejoin="round">${n.paths.map(d=>`<path d="${escape(d)}"/>`).join('')}</svg>`;
  s=await penpot.createShapeFromSvg(svg);parent.appendChild(s);
 }else if(n.kind==='check'){
  s=penpot.createBoard();parent.appendChild(s);s.clipContent=false;s.borderRadius=n.radio?999:4;fill(s,{color:n.checked?'#49EEFF':'#24222C',opacity:1});s.strokes=[{strokeColor:'#7F778E',strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];
 }else{s=penpot.createBoard();parent.appendChild(s);s.clipContent=false;await style(s,n);}
 s.name=n.name;position(s,n,dx,dy);if(n.action)s.setPluginData('agd-r2-action',n.action);
 for(const c of n.children||[])await draw(s,c,dx,dy,instances);
 // ponytail: complex panels retain measured positions; add flexible layouts after native render verification.
 if(n.layout&&s.children.length===1&&n.children[0].kind==='text'){
  const f=s.addFlexLayout();f.dir='row';f.horizontalPadding=n.layout.paddingX;f.verticalPadding=n.layout.paddingY;f.alignItems='center';f.justifyContent='center';f.horizontalSizing='fix';f.verticalSizing='fix';
  s.applyToken(dimension(A.spaces,'spacing','space',n.layout.paddingX),['paddingLeft','paddingRight']);s.applyToken(dimension(A.spaces,'spacing','space',n.layout.paddingY),['paddingTop','paddingBottom']);
  s.children[0].layoutChild.horizontalSizing='fill';s.children[0].align='center';
 }
 return s;
}
const cp=A.pages.components||penpotUtils.getPageByName('AGD R2 Draft / Components')||penpot.createPage();cp.name='AGD R2 Draft / Components';A.pages.components=cp;
await penpot.openPage(cp);
for(const n of nodes.filter(n=>n.component)){
 const k=componentKey(n);if(A.components[k])continue;
 const y=cp.root.children.reduce((b,s)=>Math.max(b,s.y+s.height),0)+24;
 const s=await draw(cp.root,n,32-n.x,y-n.y,false);const c=penpot.library.local.createComponent([s]);c.name=n.component+' / '+Object.keys(A.components).length;c.path='AGD R2 Draft';A.components[k]=c;
}
const page=A.pages[pageName]||existing||penpot.createPage();page.name=pageName;A.pages[pageName]=page;
await penpot.openPage(page);
const y=page.root.children.reduce((b,s)=>Math.max(b,s.y+s.height),0)+80;
const board=penpot.createBoard();board.name='AGD R2 / '+scene.key+' / 未承認・合成';board.resize(scene.width,scene.height);board.x=0;board.y=y;board.clipContent=true;board.showInViewMode=true;board.setPluginData('agd-r2-key',scene.key);board.setPluginData('agd-r2-source-hash',scene.sourceHash);page.root.appendChild(board);fill(board,{color:scene.palette.canvas,opacity:1});
for(const n of scene.children)await draw(board,n,0,y);
A.inProgress=null;
return {key:scene.key,id:board.id,nodes:nodes.length,components:Object.keys(A.components).length,status:'draft — native visual/geometry/prototype checks still required'};
