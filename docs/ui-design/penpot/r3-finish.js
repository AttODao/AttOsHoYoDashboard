// Additive native R3 prototype links. Core scenes plus optional language/theme samples.
// Inventory: /tmp/agd-r3/manifest.json (agd-r3-local-synthetic).
// Never changes components, visual content, R2 assets, or existing interactions.
const A=storage.agdR3;
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong Penpot file; no mutation');
if(!A)throw Error('Bind the R3 draft first');
if(A.inProgress)throw Error('R3 import interrupted or still running; inspect before finishing');
const commonKeys=['login','dashboard','setup','recovery','settings','settings-bonus','settings-link','settings-security','settings-delete','admin','unknown','queued','setup-cookie','setup-pending','menu','menu-admin','admin-created','admin-recovery','admin-reissued','admin-deletion','admin-deleted'];
const plans=[];
// Preflight the whole inventory before adding anything; do not finish a partial import.
for(const [label,width] of [['Desktop',1440],['Mobile',390]]){
 const pages=penpot.currentFile.pages.filter(p=>p.name==='AGD R3 Draft / '+label);
 if(pages.length!==1)throw Error('Expected one R3 '+label+' page');
 const page=pages[0],byKey=new Map(),expected=new Set([...commonKeys,...(width===390?['dashboard-starrail']:[])].map(k=>k+'-'+width));
 for(const board of page.root.children){
  const key=board.getPluginData('agd-r3-key');if(!key)continue;
  if((!expected.has(key)&&!/^(?:dashboard|menu)-(?:en-dark|en-light|ja-light)-(?:390|1440)$|^(?:login|settings-security)-en-light-(?:390|1440)$/.test(key))||byKey.has(key)||board.width!==width)throw Error('Unexpected, duplicate, or wrong-width R3 board: '+key);
  byKey.set(key,board);
 }
 for(const key of expected)if(!byKey.has(key))throw Error('Missing R3 board: '+key);
 plans.push({page,width,byKey});
}
const entries=(s,ancestors=[])=>[{shape:s,ancestors},...(s.children||[]).flatMap(c=>entries(c,[...ancestors,s]))];
const modalKeys=new Set(['admin-created','admin-recovery','admin-reissued','admin-deletion']);
const restrictedKeys=new Set(['setup','setup-cookie','setup-pending','recovery']);
const settingsTargets={'settings-uids':'settings','settings-bonus':'settings-bonus','settings-link':'settings-link','settings-security':'settings-security','settings-delete':'settings-delete'};
let linkcount=0,flowcount=0,preserved=0,blocked=0;
for(const {page,width,byKey} of plans){
 // Native objects may be inspected from any page, but only mutated on their owning page.
 await penpot.openPage(page);
 for(const [fullKey,board] of byKey){
  const rawKey=fullKey.slice(0,-('-'+width).length),variant=rawKey.match(/-(en-dark|en-light|ja-light)$/)?.[0]||'',key=variant?rawKey.slice(0,-variant.length):rawKey;
  const menuOpen=key==='menu'||key==='menu-admin';
  const admin=key==='admin'||key.startsWith('admin-')||key==='menu-admin';
  for(const {shape:s,ancestors} of entries(board)){
   const action=s.getPluginData('agd-r3-action');if(!action)continue;
   const lineage=[...ancestors,s];
   if(lineage.some(n=>n.hidden||n.getPluginData('agd-r3-disabled')==='1'||n.component?.()?.name?.includes('Disabled'))){blocked++;continue;}
   // Neither form dialogs nor drawer backdrops expose the covered page's controls.
   if(modalKeys.has(key)&&!lineage.some(n=>n.name==='dialog / confirmation'))continue;
   if(menuOpen&&!lineage.some(n=>n.name==='dialog / account-menu'||n.name==='Dialog backdrop'))continue;
   // Existing human interactions on this target, its contents, or a container win.
   if(lineage.some(n=>(n.interactions||[]).length)||entries(s).some(({shape:n})=>(n.interactions||[]).length)){preserved++;continue;}
   let target,targetVariant=variant;
   if(menuOpen&&key!=='menu-admin'&&(action==='toggle-theme'||action.startsWith('language-'))){
    const lang=action.startsWith('language-')?action.slice(9):variant.startsWith('-en')?'en':'ja';
    const mode=action==='toggle-theme'?(variant.endsWith('light')?'dark':'light'):(variant.endsWith('light')?'light':'dark');
    target=lang==='ja'&&mode==='dark'?'menu':'menu-'+lang+'-'+mode;targetVariant='';
   }else if(menuOpen&&action==='close-menu')target=key==='menu-admin'?'admin':!variant&&width===390?'dashboard-starrail':'dashboard';
   else if(restrictedKeys.has(key)){
    // No settings/menu/admin escapes before setup completion; recovery has no Cookie step.
    if(action==='logout')target='login';
    else if(action==='setup-password'&&key==='setup')target='setup-cookie';
    else if(action==='setup-password'&&key==='recovery')target='dashboard';
    else if(action==='setup-cookie'&&key==='setup-cookie')target='setup-pending';
    else if(action==='setup-verify'&&key==='setup-pending')target='dashboard';
   }else if(modalKeys.has(key)){
    if(action==='cancel-delete')target='admin';
    else if(action==='accept-admin-reset-demo_inactive'&&key==='admin-recovery')target='admin-reissued';
    else if(action==='accept-admin-delete'&&key==='admin-deletion')target='admin-deleted';
    // copy-generated intentionally has no link: no clipboard capability is simulated.
   }else{
    if(action==='menu'){
     // The captured mobile ordinary menu overlays Star Rail, not Genshin.
     if(key==='menu')target=width===390?'dashboard-starrail':'dashboard';
     else if(key==='menu-admin')target='admin';
     else target=admin?'menu-admin':'menu';
    }else if(action==='logout')target='login';
    else if(action==='sign-in'&&key==='login')target='dashboard';
    else if(action==='dashboard')target='dashboard';
    else if(action==='admin'&&admin)target='admin';
    else if(action==='settings'||action==='uids')target='settings';
    else if(action==='link')target='settings-link';
    else if(settingsTargets[action])target=settingsTargets[action];
    else if(width===390&&['dashboard','dashboard-starrail','menu'].includes(key)&&action==='game-genshin')target='dashboard';
    else if(width===390&&['dashboard','dashboard-starrail','menu'].includes(key)&&action==='game-starrail')target='dashboard-starrail';
    else if(['settings-bonus','unknown'].includes(key)&&['run-genshin','run-starrail'].includes(action))target='queued';
    else if(['admin','menu-admin'].includes(key)){
     if(action==='admin-create')target='admin-created';
     // Captured confirmation/results target demo_inactive; never impersonate demo_player.
     else if(action==='admin-reset-1')target='admin-recovery';
     else if(action==='admin-delete-1')target='admin-deletion';
    }
    // No self-deletion acceptance board, passkey-login board, logs API, save/result,
    // switch matrix, or refresh result is captured: leave those actions unlinked.
   }
   const destination=target&&byKey.get(target+targetVariant+'-'+width);
   if(!destination||destination.id===board.id)continue;
   s.addInteraction('click',{type:'navigate-to',destination,preserveScrollPosition:false});linkcount++;
  }
 }
 for(const [name,start] of [['Login','login'],['Initial Setup','setup'],['Recovery','recovery'],['Daily','dashboard'],['Admin','admin']]){
  const flowName='AGD R3 / '+name;
  if(!page.flows.some(f=>f.name===flowName)){page.createFlow(flowName,byKey.get(start+'-'+width));flowcount++;}
 }
}
return {
 linkcount,validationerrors:penpot.currentFile.validate().length,flowsAdded:flowcount,existingInteractionsPreserved:preserved,disabledOrHiddenSkipped:blocked,
 limitations:'Static frame navigation only: no authentication, form entry, password generation, clipboard, API outcomes, or automation simulation. Initial setup ends at the shared ordinary dashboard; first-time hidden UIDs are not simulated or changed. Admin role is not carried into shared ordinary settings/dashboard fixtures. Only demo_inactive has captured admin confirmations; self-deletion acceptance and passkey-login are absent. Backend password/current-password contracts are unchanged.'
};
