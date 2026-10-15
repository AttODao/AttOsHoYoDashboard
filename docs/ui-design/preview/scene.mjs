// Runs only in the local synthetic preview through page.evaluate, never on live UI.
export function captureScene(key){
 if(!document.querySelector('#preview-readonly')||!document.title.startsWith('AGD —'))throw Error('Not the AGD synthetic preview');
 const header=document.querySelector('.topbar'),main=document.querySelector('main'),dialog=document.querySelector('dialog[open]');
 const origin=dialog?0:header.getBoundingClientRect().top;
 const hex=value=>{if(value==='transparent')return {color:'#000000',opacity:0};const m=value.match(/^rgba?\(([^)]+)\)$/);if(!m)throw Error('Unsupported CSS color: '+value);const n=m[1].split(',').map(Number);return {color:'#'+n.slice(0,3).map(v=>Math.round(v).toString(16).padStart(2,'0')).join('').toUpperCase(),opacity:n[3]??1};};
 const box=r=>({x:r.x,y:r.y-origin,width:r.width,height:r.height});
 const paint=(e,c)=>{const widths=['Top','Right','Bottom','Left'].map(s=>parseFloat(c['border'+s+'Width'])||0);return {fill:hex(c.backgroundColor),stroke:hex(c.borderTopColor),strokeWidth:widths.every(w=>w===widths[0])?widths[0]:0,radius:parseFloat(c.borderTopLeftRadius)||0};};
 const type=c=>({family:c.fontFamily.split(',')[0].replace(/["']/g,'').trim(),size:parseFloat(c.fontSize),weight:c.fontWeight,lineHeight:parseFloat(c.lineHeight)/parseFloat(c.fontSize)||1.6,align:c.textAlign,color:hex(c.color),...(parseFloat(c.letterSpacing)?{letterSpacing:parseFloat(c.letterSpacing)}:{})});
 let id=0;
 function node(e){
  if(e.nodeType===Node.TEXT_NODE){
   const text=e.textContent.replace(/\s+/g,' ').trim();if(!text)return null;
   const r=document.createRange();r.selectNodeContents(e);const bounds=r.getBoundingClientRect();if(!bounds.width||!bounds.height)return null;
   const parent=e.parentElement,c=getComputedStyle(parent),p=parent.getBoundingClientRect();
   const width=/^(H[1-6]|P)$/.test(parent.tagName)?Math.max(bounds.width,p.right-parseFloat(c.paddingRight||0)-bounds.x):bounds.width;
   return {kind:'text',id:'n'+id++,name:'Text / '+text.slice(0,32),text,...box(bounds),width:Math.ceil(width)+4,type:type(c)};
  }
  if(e.nodeType!==Node.ELEMENT_NODE)return null;
  const c=getComputedStyle(e);if(c.display==='none'||c.visibility==='hidden'||c.opacity==='0'||e.closest('.review-tools'))return null;
  if(e.tagName!=='DETAILS'&&e.closest('details:not([open])')&&!e.closest('summary'))return null;
  const r=e.getBoundingClientRect();if(!r.width||!r.height)return null;
  if(e.tagName.toLowerCase()==='svg'){
   const paths=Array.from(e.querySelectorAll('path')).map(p=>p.getAttribute('d'));
   if(!paths.length||paths.length>8||r.width>64||r.height>64)throw Error('Only authored small icons are supported');
   return {kind:'icon',id:'n'+id++,name:'Icon',...box(r),paths,color:hex(c.color),strokeWidth:parseFloat(c.strokeWidth)||1.8};
  }
  const tag=e.tagName.toLowerCase(),label=e.labels?.[0]?.textContent.trim()||e.closest('label')?.textContent.trim()||e.getAttribute('aria-label')||tag;
  const heading=e.matches('main,section,.panel,.auth-copy')?e.querySelector('h1,h2,h3')?.textContent:null;
  const name=e.id||heading||(label===tag?e.className:label)||tag;
  const out={kind:'board',id:'n'+id++,name:tag+' / '+name,...box(r),paint:paint(e,c),children:[]};
  if(c.backgroundImage!=='none'){
   const angle=c.backgroundImage.match(/^linear-gradient\(([-\d.]+)deg,/);
   if(!angle)throw Error('Only two-stop linear surface gradients are supported');
   const stops=[...c.backgroundImage.matchAll(/rgba?\([^)]+\)|transparent/g)].map(m=>hex(m[0]));
   if(stops.length!==2)throw Error('Only two-stop gradients are supported');
   out.paint.gradient={angle:Number(angle[1]),stops};
  }
  if(out.paint.strokeWidth===0)for(const side of ['Top','Right','Bottom','Left']){
   const w=parseFloat(c['border'+side+'Width'])||0;if(!w)continue;
   out.children.push({kind:'board',id:'n'+id++,name:'Divider / '+side,x:out.x+(side==='Right'?out.width-w:0),y:out.y+(side==='Bottom'?out.height-w:0),width:['Top','Bottom'].includes(side)?out.width:w,height:['Left','Right'].includes(side)?out.height:w,paint:{fill:hex(c['border'+side+'Color']),stroke:{color:'#000000',opacity:0},strokeWidth:0,radius:0},children:[]});
  }
  if(tag==='input'||tag==='select'){
   const inputType=e.hasAttribute('data-password')?'password':e.type||'select';out.inputType=inputType;out.name='Input / '+label.slice(0,48);out.disabled=!!e.disabled;
   if(inputType==='checkbox'||inputType==='radio'){
    out.kind='check';out.checked=e.checked;out.radio=inputType==='radio';out.color=hex(c.accentColor);
    if(e.dataset.automatic)out.action='automatic-'+e.dataset.automatic;
    if(e.name==='method')out.action='method-'+e.getAttribute('value');
    if(out.checked)out.children.push(out.radio?{kind:'board',id:'n'+id++,name:'Radio selected indicator',x:out.x+out.width*.28,y:out.y+out.height*.28,width:out.width*.44,height:out.height*.44,paint:{fill:{color:'#0B0B10',opacity:1},stroke:{color:'#000000',opacity:0},strokeWidth:0,radius:999},children:[]}:{kind:'icon',id:'n'+id++,name:'Checkbox checked indicator',...box(r),paths:['M5 12l4 4L19 6'],color:{color:'#0B0B10',opacity:1},strokeWidth:2.4});
    return out;
   }
   // Never inspect credential input values, even in the synthetic preview.
   let text=' ';
   const fixture=new URLSearchParams(location.search).get('fixture');
   if(tag==='select')text=e.selectedOptions[0]?.textContent||' ';
   else if(inputType==='time')text='06:00';
   else if(/Cookie/.test(label))text=' ';
   else if(e.id==='new-username'||/新しいユーザー名/.test(label))text='demo_new';
   else if(/対象名|target username/i.test(label))text=document.querySelector('#confirmation-title')?.textContent.includes('demo_inactive')?'demo_inactive':'demo_player';
   else if(/ユーザー名|username/i.test(label))text=fixture==='admin'?'demo_admin':fixture==='provisional'?'demo_pending':'demo_player';
   out.children.push({kind:'text',id:'n'+id++,name:'Value / synthetic',text,x:out.x+14,y:out.y+12,width:out.width-28-(e.hasAttribute('data-password')?48:0),height:24,type:type(c)});
   if(tag==='select')out.children.push({kind:'icon',id:'n'+id++,name:'Select indicator',x:out.x+out.width-26,y:out.y+out.height/2-6,width:12,height:12,paths:['M4 8l8 8 8-8'],color:hex(c.color),strokeWidth:1.8});
   if(!e.disabled)out.component='Input/'+(tag==='select'?'Select':inputType==='password'?'Password':'Default');
   return out;
  }
  if(tag==='button'||tag==='summary'){
   out.name='Button / '+(e.textContent.trim()||e.getAttribute('aria-label')||'');out.disabled=!!e.disabled;
   out.action=e.dataset.action||(tag==='summary'?'menu':e.dataset.gameTab?'game-'+e.dataset.gameTab:e.dataset.settingsTab?'settings-'+e.dataset.settingsTab:undefined);
   // Icon-specific controls keep separate masters; copy structures stay immutable.
   out.component='Button/'+(e.disabled?'Disabled':e.classList.contains('password-eye')?'Visibility':e.classList.contains('menu')?'Menu':tag==='summary'?'Menu':e.closest('.menu-drawer nav')?'Nav-'+out.action:e.classList.contains('primary')?'Primary':e.classList.contains('danger')?'Danger':'Secondary');
   if(!e.querySelector('svg'))out.layout={paddingX:parseFloat(c.paddingLeft)||0,paddingY:parseFloat(c.paddingTop)||0};
  }
  if(tag==='progress'){
   out.paint.fill=hex(c.backgroundColor);out.paint.radius=4;
   out.children.push({kind:'board',id:'n'+id++,name:'Resource progress',x:out.x,y:out.y,width:out.width*e.value/e.max,height:out.height,paint:{fill:{color:getComputedStyle(e).getPropertyValue(e.closest('.starrail')?'--lavender':'--mint').trim().toUpperCase(),opacity:1},stroke:{color:'#000000',opacity:0},strokeWidth:0,radius:4},children:[]});
  }
  if(e.classList.contains('switch')){out.action='automatic-'+e.querySelector('input').dataset.automatic;out.checked=e.querySelector('input').checked;out.component='Switch/'+(out.checked?'On':'Off');}
  for(const child of e.childNodes){const n=node(child);if(n)out.children.push(n);}
  if(e.classList.contains('switch-track')){
   const checked=e.parentElement.querySelector('input').checked;
   out.children.push({kind:'board',id:'n'+id++,name:'Switch thumb',x:out.x+4+(checked?20:0),y:out.y+4,width:20,height:20,paint:{fill:{color:checked?'#0B0B10':'#B7B2C4',opacity:1},stroke:{color:'#000000',opacity:0},strokeWidth:0,radius:10},children:[]});
  }
  return out;
 }
 // CSS z-index: the header menu must paint above the main content.
 const children=[node(main),node(header)].filter(Boolean);
 if(dialog){children.push({kind:'board',id:'n'+id++,name:'Dialog backdrop',...(dialog.id==='account-menu'?{action:'close-menu'}:{}),x:0,y:0,width:innerWidth,height:innerHeight,paint:{fill:{color:'#000000',opacity:.7},stroke:{color:'#000000',opacity:0},strokeWidth:0,radius:0},children:[]});children.push(node(dialog));}
 const vars=getComputedStyle(document.documentElement),palette={};
 for(const k of ['canvas','surface','elevated','text','muted','line','decorative','pink','cyan','mint','lavender','warning','danger','on-accent'])palette[k]=vars.getPropertyValue('--'+k).trim().toUpperCase();
 return {schema:1,source:'agd-r3-local-synthetic',language:document.documentElement.lang,theme:document.documentElement.dataset.theme||'dark',key,width:innerWidth,height:dialog?innerHeight:Math.ceil(Math.max(...children.map(n=>n.y+n.height)))+32,palette,children};
}
