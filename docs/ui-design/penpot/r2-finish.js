// Additive prototype wiring and narrow corrections on AGD R2 draft pages only.
const A=storage.agdR2;if(!A)throw Error('Bind the R2 draft first');
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong file');
const pages=penpot.currentFile.pages.filter(p=>/^AGD R2 Draft \/ (Desktop|Mobile)$/.test(p.name));
const all=s=>[s,...(s.children||[]).flatMap(all)];let links=0,hiddenMenus=0,dividers=0;
A.zeroBorder??=A.set.addToken({type:'borderWidth',name:'agd.r2.border.none',value:'0'});
for(const page of pages){
 await penpot.openPage(page);
 const boards=page.root.children.filter(b=>b.getPluginData('agd-r2-key')),byKey=Object.fromEntries(boards.map(b=>[b.getPluginData('agd-r2-key'),b]));
 for(const b of boards){
  const key=b.getPluginData('agd-r2-key').replace(/-(1440|390)$/,''),suffix='-'+b.width;
  const header=b.children.find(s=>s.name.startsWith('header /'));
  if(header){header.setParentIndex(b.children.length-1);for(const s of all(header))if(s.name.startsWith('nav / アカウントメニュー')){s.hidden=!key.startsWith('menu');if(s.hidden)hiddenMenus++;}}
  // The first import treated CSS border-top as a four-sided border. Preserve its content.
  for(const s of all(b))if(s.name==='div / expeditions'&&!s.children.some(c=>c.name==='Divider / Top')){
   s.strokes=[{strokeColor:'#38333F',strokeWidth:0,strokeStyle:'solid',strokeAlignment:'inner'}];s.applyToken(A.zeroBorder,['strokeWidth']);
   const line=penpot.createRectangle();line.name='Divider / Top';line.resize(s.width,1);s.appendChild(line);line.x=s.x;line.y=s.y;line.fills=[{fillColor:'#38333F',fillOpacity:1}];line.setParentIndex(0);dividers++;
  }
  const admin=all(b).some(s=>s.type==='text'&&s.characters==='demo_admin');
  for(let s of all(b)){
   const action=s.getPluginData('agd-r2-action');if(!action||s.getPluginData('agd-r2-disabled')==='1'||s.component?.()?.name.includes('Disabled'))continue;
   if(s.hidden)continue;
   let target;
   if(action==='menu')target=admin?'menu-admin':'menu';
   else if(action==='logout')target='login';
   else if(action==='sign-in')target='dashboard';
   else if(action==='method-password')target='login';
   else if(action==='method-passkey')target='login-passkey';
   else if(action==='setup-password')target='setup-cookie';
   else if(action==='setup-cookie')target='setup-pending';
   else if(action==='setup-verify')target='dashboard-new-uids';
   else if(action==='automatic-genshin'&&['dashboard','dashboard-on'].includes(key))target=key==='dashboard'?'dashboard-on':'dashboard';
   else if(action==='game-starrail'&&key==='dashboard-on')target='dashboard-starrail';
   else if(action==='game-genshin'&&key==='dashboard-starrail')target='dashboard-on';
   else if(['dashboard','settings','admin'].includes(action))target=action;
   else if(action==='link'||action==='uids'||action.startsWith('section-'))target='settings';
   else if(action==='refresh')target='loading';
   else if(action.startsWith('run-'))target='queued';
   else if(action==='delete-self')target='deleted';
   else if(action.startsWith('admin-reset-'))target='admin-recovery';
   else if(action.startsWith('admin-delete-'))target='admin-deletion';
   else if(action==='accept-admin-delete')target='admin-deleted';
   else if(action==='cancel-delete'||action==='accept-admin-reset')target='admin';
   if(!target||!byKey[target+suffix])continue;
   // A native label, rather than its tiny radio indicator, is the click target.
   if(action.startsWith('method-')&&s.parent?.name.startsWith('label /'))s=s.parent;
   if(!s.interactions.length){s.addInteraction('click',{type:'navigate-to',destination:byKey[target+suffix],preserveScrollPosition:false});links++;}
  }
 }
 for(const [name,start]of [['Login','login'],['Setup','setup'],['Daily','dashboard'],['Admin','admin']])if(!page.flows.some(f=>f.name==='AGD R2 / '+name)&&byKey[start+'-'+(page.name.endsWith('Mobile')?390:1440)])page.createFlow('AGD R2 / '+name,byKey[start+'-'+(page.name.endsWith('Mobile')?390:1440)]);
}
return {linksAdded:links,closedMenusHidden:hiddenMenus,oneSideBordersCorrected:dividers,fileErrors:penpot.currentFile.validate().length,note:'Representative static flows only; form entry, section scroll, save notifications and the full switch-state matrix are not simulated.'};
