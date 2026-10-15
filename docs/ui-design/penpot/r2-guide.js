// Additive native guide only. Run as an async function body after R2 imports finish.
// No asset creation, instance-child overrides, images, or changes to R1/import markers.
if(penpot.currentFile.id!=='129977ce-683b-806a-8008-c9d0bedd3212')throw Error('Wrong Penpot file; no mutation');
const A=storage.agdR2;
if(!A?.typo||!A?.tokens||!A?.components)throw Error('Existing R2 assets required; no mutation');
if(A.inProgress)throw Error('Import interrupted or running; inspect first; marker preserved');
const pageName='AGD R2 Draft / Guide',marker='agd-r2-guide';
const matches=penpot.currentFile.pages.filter(p=>p.name===pageName);
if(matches.length>1)throw Error('Duplicate Guide pages; preserve human work');
const existing=penpotUtils.getPageByName(pageName);
if(existing?.root.children.some(s=>s.getPluginData(marker)))throw Error('Guide marker exists (complete or partial); no rerun');
if(existing?.root.children.length)throw Error('Guide page already has content; preserve human work / no rerun');
// Inspect actual keys, including optional tracking; never guess a line-height key.
const types=Object.entries(A.typo).map(([key,ref])=>{
 const [family,size,weight]=key.split('/');return {key,ref,family,size:Number(size),weight:Number(weight)};
});
const body=types.filter(t=>t.family==='Noto Sans JP'&&t.size===16).sort((a,b)=>Math.abs(a.weight-400)-Math.abs(b.weight-400))[0];
const heading=types.filter(t=>t.family==='Noto Sans JP'&&t.size>=22&&t.size<=36).sort((a,b)=>Math.abs(a.size-28)-Math.abs(b.size-28)||Math.abs(a.weight-600)-Math.abs(b.weight-600))[0];
const brand=types.filter(t=>t.family==='Orbitron'&&t.size>=24&&t.size<=32).sort((a,b)=>Math.abs(a.size-26)-Math.abs(b.size-26)||Math.abs(a.weight-700)-Math.abs(b.weight-700))[0];
if(!body||!heading||!brand)throw Error('Required existing body, heading, or Orbitron Typography missing');
const roles=['Input/Default','Input/Password','Button/Primary','Button/Secondary'];
const controls=roles.map(role=>{
 const entry=Object.entries(A.components).find(([key])=>key.startsWith(role+' / '));
 if(!entry)throw Error('Missing native component: '+role);
 return {role,component:entry[1]};
});
// Canonical R2 palette: DESIGN.md / preview/style.css, not the R1 token file.
const palette=[
 ['canvas','#0C0B0E'],['surface','#18171D'],['elevated','#24222C'],
 ['text','#F8F7FC'],['muted','#B7B2C4'],['line','#7F778E'],
 ['decorative','#38333F'],['pink','#FF58EC'],['cyan','#49EEFF'],
 ['on-accent','#0B0B10'],['mint','#93F5CE'],['lavender','#D5B7FF'],
 ['warning','#FFD585'],['danger','#FFB2BD']
];
let texts=0,swatches=0,instances=0,groups=0,colorBindings=0,sizeBindings=0;
function fill(s,color){
 s.fills=[{fillColor:color,fillOpacity:1}];
 if(A.tokens[color]){s.applyToken(A.tokens[color],['fill']);colorBindings++;}
}
function text(parent,name,copy,x,y,w,h,type=body,color='#F8F7FC'){
 const s=penpot.createText(copy);parent.appendChild(s);s.name=name;
 s.applyTypography(type.ref);s.growType='auto-height';s.align='left';
 if(A.sizes?.[type.size]){s.applyToken(A.sizes[type.size],['fontSize']);sizeBindings++;}
 fill(s,color);s.resize(w,h);s.x=parent.x+x;s.y=parent.y+y;texts++;return s;
}
function section(name,x,y,w,h){
 const s=penpot.createBoard();board.appendChild(s);s.name=name;
 s.resize(w,h);s.x=x;s.y=y;s.clipContent=false;s.strokes=[];fill(s,'#18171D');groups++;return s;
}
// Only an empty Guide page is reusable. Mark the board before drawing so partial
// failures remain visible and the next run stops rather than replacing anything.
const page=existing||penpot.createPage();page.name=pageName;
await penpot.openPage(page);
const board=penpot.createBoard();board.name='AGD R2 / Guide / 未承認・合成';
board.resize(1536,2384);board.x=0;board.y=0;board.clipContent=false;board.strokes=[];
board.setPluginData(marker,'building-v1');page.root.appendChild(board);fill(board,'#0C0B0E');
text(board,'Brand','AGD',48,40,160,64,brand,'#FF58EC');
text(board,'Title','R2 デザインガイド',240,40,1000,72,heading);
text(board,'Draft status','未承認の制作案。本番 Dioxus 実装ではありません。',48,120,1440,40,body,'#B7B2C4');

const rules=section('Guide / 画面・状態の約束',48,200,704,1184);
text(rules,'Section title','画面・状態の約束',24,24,656,64,heading);
let ruleY=104;
function rule(title,copy){
 text(rules,title+' / title',title,24,ruleY,656,32,body,'#49EEFF');
 const height=copy.split('\n').length*32+8;
 text(rules,title+' / guidance',copy,24,ruleY+40,656,height);
 ruleY+=40+height+24;
}
rule('画面構成','AGD。常設サイドバーなし。上部メニューは1つ。\nDashboard とゲーム別の自動化にアクセスし、\nその他の作業は Settings に集約します。');
rule('共通ユーザー名','パスワード／パスキーのログイン名は共通です。');
rule('仮セットアップは隔離','パスワード変更 → Cookie 保存 → 本人確認。\n確認完了まで移動は Logout のみ。\nCookie 保存は本人確認完了ではありません。\n完了後も新規 UID は非表示・Empty になり得ます。');
rule('受付と結果を分ける','queued／202 受付は報酬の成功ではありません。\nPOST 結果が unknown のときは状態を確認し、\n無条件の再送を促しません。');
rule('削除の受付は完了ではない','他ユーザーを削除する管理者のセッションは維持。\n削除対象のセッション／パスキーを失効します。\n「削除受付」を「削除完了」と表示しません。');
rule('管理者ログは未提供','一覧 API は現在ありません。架空の実ログや\n有効な取得操作を示しません。');
rule('合成データのみ','元データのセルもサンプルも合成値のみ。\n実 Cookie・パスワード・アカウント秘密は入力禁止。');

const colors=section('Guide / Color tokens',784,200,704,768);
text(colors,'Section title','配色 / Color tokens',24,24,656,64,heading);
text(colors,'Color binding note','色は既存 token に接続（存在する場合）。\n暗い区切り線を入力境界に転用しません。',24,96,656,72,body,'#B7B2C4');
for(const [i,[name,hex]]of palette.entries()){
 const x=24+(i%2)*336,y=192+Math.floor(i/2)*76;
 const s=penpot.createRectangle();colors.appendChild(s);s.name='Swatch / '+name;
 s.resize(48,48);s.x=colors.x+x;s.y=colors.y+y;
 // These intentional borders keep near-black swatches visible. No stroke token.
 s.strokes=[{strokeColor:'#7F778E',strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];
 fill(s,hex);swatches++;
 text(colors,name+' / label',name+'\n'+hex,x+64,y,248,68);
}

const typography=section('Guide / Typography assets',784,1000,704,384);
text(typography,'Section title','Typography assets',24,24,656,64,heading);
text(typography,'Body sample','本文 16px / 日本語の読みやすさを優先',24,112,656,48);
text(typography,'Heading sample','見出し / 状態を明確に',24,184,656,64,heading);
text(typography,'Orbitron sample','AGD 202',24,264,656,64,brand,'#49EEFF');

const samples=section('Guide / Native component instances',48,1416,1440,432);
text(samples,'Section title','共通操作 / Native instances',24,24,1392,64,heading);
text(samples,'Control note','各サンプルは既存 master のコピー。幅・高さ 44px 以上、通常高さ 48px。\n合成ラベルの静的見本です。送信・保存・検証の実行はしません。',24,104,1392,80,body,'#B7B2C4');
for(const [i,{role,component}]of controls.entries()){
 const x=24+(i%2)*712,y=208+Math.floor(i/2)*108;
 text(samples,role+' / caption',role,x,y,680,32,body,'#49EEFF');
 const s=component.instance();samples.appendChild(s);instances++;
 // Preserve dimensions and topology; no insert/remove or label overrides in copies.
 s.x=samples.x+x;s.y=samples.y+y+40;
 // Initial sizing only; use r2-refresh.js across separate RPCs before export.
 const refresh=t=>{if(t.type==='text'){const size=t.fontSize;t.fontSize=String(Number(size)+1);t.fontSize=size;if(A.sizes?.[Number(size)])t.applyToken(A.sizes[Number(size)],['fontSize']);}for(const child of t.children||[])refresh(child);};refresh(s);
 if(!Number.isFinite(s.width)||!Number.isFinite(s.height)||s.width<44||s.height<44||s.width>680||s.height>64){
  throw Error('Sample dimensions need human review: '+role+'; partial Guide preserved');
 }
}

const editing=section('Guide / 編集と検証の境界',48,1880,1440,392);
text(editing,'Section title','編集と検証の境界',24,24,1392,64,heading);
text(editing,'Ownership',
 '共通操作は component masters を編集します。色 token・Typography asset の変更は Apply／Update も確認します。\nSDK の値変更だけで既存図形へ自動同期されるとは保証しません。未反映時は対象を再適用してください。',
 24,112,1392,88);
text(editing,'Verification',
 '複雑な計測レイアウトは、人間による Auto Layout と Native render の確認が引き続き必要です。\n共有編集の伝播・日本語の折返し・44px の操作領域も、実際の Penpot 上で確認してください。\nこのガイドの構文成功だけでは、Native 描画・共有編集の検証完了や本番採用を意味しません。',
 24,224,1392,128,body,'#B7B2C4');
text(board,'Footer','Draft / 未承認 — 実描画・共有編集・人間のレビュー待ち',48,2312,1440,48,body,'#FFD585');
board.setPluginData(marker,'complete-v1');
return {id:board.id,counts:{texts,swatches,instances,groups,colorBindings,sizeBindings},typographyKeys:{body:body.key,heading:heading.key,brand:brand.key},status:'draft — native render and shared editing not verified'};
