// Finalize existing AI drafts before human editing. Never replay on approved/edited boards.
const A=storage.attos;
const labels={
 'ダッシュボード':'dashboard','HoYoLAB連携':'link','UID表示設定':'uids','ログインボーナス':'rewards','セキュリティ':'security',
 'ログイン':'onboarding','パスキーを使う':'passkey-prompt','メニュー':'menu','ログアウト':'login',
 'API仕様を見る':'docs','ログインへ戻る':'login','ログイン画面へ':'login','パスワードでログイン':'login',
 'Cookieを保存':'pending','本人確認を実行':'uids','設定状態を更新':'pending','ゲームUIDを同期':'uids',
 'UID表示を設定':'uids','UID表示を設定する':'uids','ダッシュボードへ':'dashboard','ダッシュボードへ戻る':'dashboard',
 'ボーナス設定と履歴':'rewards','設定を保存':'reward-saved','今すぐ実行':'reward-queued','実行の詳細を見る':'reward-detail',
 '履歴を更新':'rewards','履歴へ戻る':'rewards','設定と履歴へ戻る':'rewards',
 'セキュリティ設定':'security','セキュリティへ':'security','セキュリティへ戻る':'security','アカウント削除':'delete','アカウント削除へ':'delete',
 'パスキーを登録':'passkey-prompt','このパスキーを削除':'passkey-deleted','中断時の表示を見る':'passkey-cancel','もう一度試す':'passkey-prompt',
 'ユーザーを作成':'admin','demo_player を管理':'admin-detail','demo_inactive を管理':'admin-inactive',
 'ユーザーを無効化':'admin-disabled','ユーザーを再有効化':'admin-reenabled','復旧用パスワードを設定':'admin-reset','ユーザー一覧へ戻る':'admin',
 '削除を要求する':'deletion-accepted','HoYoLAB連携を確認':'link',
 'ログアウトを再試行':'login','状態を再読み込み':'login','状態を再取得':'loading'
};
const results=[];
for(const [pageKey,page] of Object.entries(A.pages)){
 await penpot.openPage(page);
 const roots=page.root.children;
 let links=0,outlineFixes=0,textTokens=0;
 for(const root of roots){
  const key=root.getPluginData('attos-key')||Object.entries(A.boards).find(([k,b])=>b.id===root.id)?.[0]||'';
  const suffix=key.endsWith('-m')?'m':'d';
  const all=penpotUtils.findShapes(()=>true,root);
  for(const s of all){
   if(s.type==='text'){
    const token=A.tokens['size'+s.fontSize];if(token){s.applyToken(token,['fontSize']);textTokens++;}
    if(key.startsWith('admin-inactive')&&s.characters.includes('demo_player'))s.characters=s.characters.replaceAll('demo_player','demo_inactive');
   }
   if(s.name==='Resource track')A.fill(s,'line');
   if(s.strokes?.some(st=>st.strokeColor?.toUpperCase()==='#435269')){s.strokes=s.strokes.map(st=>({...st,strokeColor:A.colors.line}));s.applyToken(A.tokens.line,['strokeColor']);outlineFixes++;}
   const comp=s.component?.();
   if(s.type==='board'&&(comp?.name==='Secondary'&&comp.path.includes('Button'))){s.strokes=[{strokeColor:A.colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];s.applyToken(A.tokens.line,['strokeColor']);}
   if(s.name==='Input' || (s.type==='board'&&s.name.startsWith('Input/'))){s.strokes=[{strokeColor:A.colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];s.applyToken(A.tokens.line,['strokeColor']);}
   if(s.name==='Box'&&(s.fills?.[0]?.fillColor?.toUpperCase()===A.colors.elevated)){
    A.fill(s,'elevated');s.strokes=[{strokeColor:A.colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];s.applyToken(A.tokens.line,['strokeColor']);outlineFixes++;
   }
   if(s.type==='path'&&s.parent?.parent?.name==='UID selector')s.applyToken(A.tokens.text,['strokeColor']);
   if(!['01','02','03'].includes(pageKey)||s.hidden)continue;
   let target;
   if(s.name.startsWith('Nav / ')){target=s.name.slice(6)+'-'+suffix;}
   if(s.name==='UID selector')target=(key.startsWith('starrail')?'dashboard':'starrail')+'-'+suffix;
   if(s.name.startsWith('Visibility / '))target='uids-saved-'+suffix;
   if(s.name.startsWith('Button / ')){
    const label=penpotUtils.findShape(q=>q.type==='text',s)?.characters;
    if(comp?.name==='Disabled')continue;
    if(label==='パスワードを変更')target=(key.startsWith('onboarding')?'pending':'password-saved')+'-'+suffix;
    else if(label?.endsWith('の削除を要求'))target='admin-deletion-accepted-'+suffix;
    else if(labels[label])target=labels[label]+'-'+suffix;
   }
   if(target&&A.boards[target]){if(!s.interactions.length)s.addInteraction('click',{type:'navigate-to',destination:A.boards[target],preserveScrollPosition:false});links++;}
  }
  if(key==='concept-a'){
   const admin=penpotUtils.findShape(s=>s.name==='Nav / admin',root);if(admin)admin.hidden=true;
  }
  if(key.startsWith('loading')||key.startsWith('empty')||key.startsWith('error')){
   for(const n of penpotUtils.findShapes(s=>s.name.startsWith('Nav / '),root)){const selected=n.name==='Nav / dashboard';A.fill(n,selected?'primary':'rail');const t=penpotUtils.findShape(s=>s.type==='text',n);if(t)A.fill(t,selected?'onPrimary':'text');}
  }
 }
 for(const flow of page.flows){if(flow.name.startsWith('AttOs Draft / '))page.removeFlow(flow);}
 if(pageKey==='01')page.createFlow('AttOs Draft / Concepts',A.boards['concept-a']);
 if(pageKey==='02'||pageKey==='03'){
  const suffix=pageKey==='03'?'m':'d';page.createFlow('AttOs Draft / Setup '+suffix,A.boards['login-'+suffix]);page.createFlow('AttOs Draft / Daily '+suffix,A.boards['dashboard-'+suffix]);page.createFlow('AttOs Draft / Admin '+suffix,A.boards['admin-'+suffix]);
 }
 results.push({page:pageKey,links,outlineFixes,textTokens});
}
await penpot.openPage(A.pages['04']);
for(const [key,c] of Object.entries(A.components)){c.path='AttOs Draft'+(key.includes('/')?' / '+key.split('/').slice(0,-1).join(' / '):'');c.name=key.split('/').pop();}
const control=A.boards['control-states'];
if(control){
 const focus=penpotUtils.findShape(s=>s.name==='Button / Focus',control);if(focus)focus.strokes=[];
 const ring=penpotUtils.findShape(s=>s.name==='Focus offset / outer',control);
 if(ring){ring.name='Focus ring / 4px offset';ring.resize(528,56);ring.x=control.x+44;ring.y=control.y+276;ring.strokes=[{strokeColor:A.colors.text,strokeWidth:2,strokeStyle:'solid',strokeAlignment:'outer'}];ring.applyToken(A.tokens.text,['strokeColor']);}
}
await penpot.openPage(A.pages['01']);
for(const key of ['concept-a','concept-b']){
 const b=A.boards[key],game=penpotUtils.findShape(s=>s.name==='GameStatus / Genshin',b);
 if(game&&!penpotUtils.findShape(s=>s.name==='Button / 状態を再取得',game)){const refresh=A.button(game,'状態を再取得',game.width-224,32,192,'Secondary','loading-d');refresh.addInteraction('click',{type:'navigate-to',destination:A.boards['loading-d']});}
 const selector=penpotUtils.findShape(s=>s.name==='UID selector',b);if(selector){selector.strokes=[{strokeColor:A.colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];selector.applyToken(A.tokens.line,['strokeColor']);}
}
const b=A.boards['concept-b'];
if(!penpotUtils.findShape(s=>s.name==='Button / ログアウト',b)){
 const user=penpotUtils.findShape(s=>s.name==='User',b);if(user){user.x=b.x+912;user.resize(264,32);user.growType='auto-height';}
 const logout=A.button(b,'ログアウト',1200,24,200,'Secondary','login-d');logout.addInteraction('click',{type:'navigate-to',destination:A.boards['login-d']});
}
// ponytail: native copy edits can retain stale glyph caches; remove this refresh when the upstream API invalidates them.
const refreshed=[];
for(const [pageKey,page] of Object.entries(A.pages)){
 await penpot.openPage(page);
 let count=0;
 for(const s of page.findShapes({type:'text'})){
  const size=s.fontSize,grow=s.growType,w=s.width,h=s.height;
  const asset=Object.values(A.typo).find(t=>t.fontSize===size&&t.fontFamily===s.fontFamily&&t.fontWeight===s.fontWeight);
  s.fontSize=String(Number(size)+1);s.resize(w,h);s.growType=grow;s.fontSize=size;
  if(asset)s.applyTypography(asset);
  if(A.tokens['size'+size])s.applyToken(A.tokens['size'+size],['fontSize']);
  count++;
 }
 await new Promise(resolve=>setTimeout(resolve,100));
 refreshed.push({page:pageKey,count});
 if(pageKey==='03')for(const [key,root]of Object.entries(A.boards).filter(([k])=>k.endsWith('-m'))){
  const title=root.children.find(s=>s.name==='Page title'),label=root.children.find(s=>s.name==='Synthetic data label');
  if(!title||!label)continue;
  const titleBottom=Math.max(title.y+title.height,title.textBounds.y+title.textBounds.height);
  const newLabelY=titleBottom+12,delta=newLabelY-label.y;
  if(delta>1){label.y=newLabelY;for(const child of root.children)if(child.id!==title.id&&child.id!==label.id&&child.name!=='Header'&&child.y>=root.y+220)child.y+=delta;}
  const bottom=Math.max(...root.children.filter(s=>!s.hidden).map(s=>s.y+s.height));
  if(bottom>root.y+root.height-32)root.resize(root.width,Math.ceil(bottom-root.y)+32);
 }
}
return {results,refreshed,components:penpot.library.local.components.length,validationErrors:penpot.currentFile.validate().length};
