const A=storage.attos;
await penpot.openPage(A.pages['00']);
const ds=A.artboard('00','system','Orbit Folio / デザインシステム・編集ガイド',0,0,1440,2200);
A.text(ds,'Title','Orbit Folio',56,40,1200,'Display');A.text(ds,'Status','AI推奨候補・未承認。共通設定はAssets / Tokensで編集。',56,128,1280,'Body','muted');
A.text(ds,'Palette title','色の役割',56,208,1280,'Title');
const palette=[['canvas','画面背景'],['surface','情報面'],['rail','ナビゲーション'],['primary','主操作 / 選択'],['genshin','原神'],['starrail','スターレイル'],['warning','確認待ち / 不確定'],['error','失敗 / 破壊操作'],['success','完了'],['line','入力 / 非選択の境界']];
for(let i=0;i<palette.length;i++){let [key,label]=palette[i],x=56+(i%5)*268,y=288+Math.floor(i/5)*184;A.rect(ds,'Swatch / '+key,x,y,244,80,key,8);A.text(ds,'Color role / '+key,label,x,y+92,244,'Label');A.text(ds,'Token / '+key,key+' · '+A.colors[key],x,y+128,244,'Small','muted');}
A.text(ds,'Typography title','タイポグラフィ',56,720,1280,'Title');
const styles=[['Title','画面の見出し',32],['Section','セクションの見出し',20],['Body','本文・操作の説明',16],['Small','補足・取得日時',14],['Number','142 / 200',64]];
let y=800;for(const [style,sample,size] of styles){A.text(ds,'Typography / '+style,sample,56,y,820,style);A.text(ds,'Typography specification',A.typo[style].fontFamily+' / '+size+'px',960,y,424,'Body','muted');y+=style==='Number'?120:96;}
A.text(ds,'Layout title','レイアウトと操作',56,1352,1280,'Title');
A.panel(ds,'Grid and responsive',56,1424,640,[{type:'title',text:'Desktop 1440 / Mobile 390'},{type:'p',text:'Desktop: 232px rail + 56px内側余白\nMobile: 20px余白・1カラム\nフォームは上下へ再構成し、機能を隠さない。'},{type:'small',text:'余白 4 / 8 / 12 / 16 / 24 / 32 / 48 / 56\n角丸 4 / 8 / 12。ボタン・入力48px。'},{type:'p',text:'色だけに依存せず状態の文言を表示。\n連続アニメーション・点滅は使わない。'}]);
A.panel(ds,'Editing guide',720,1424,664,[{type:'title',text:'編集の入口'},{type:'p',text:'色: Tokens > attos-orbit\n文字: Assets > Typography > AttOs Draft\n部品: 04 / Componentsのマスター\n画面: 02 / Desktop または03 / Mobile'},{type:'small',text:'font-family token適用は接続プラグインでエラーになるため、書体変更はTypography assetsで行います。サイズ・配色・余白・角丸はtoken bindingで変更できます。'},{type:'p',text:'構成の変更はFlexのgap・paddingで。\nPenpotを正として人間が確定し、RSXは次の作業で実装します。'}]);
await penpot.openPage(A.pages['04']);
const atlas=A.artboard('04','state-atlas','状態一覧 / 処理と安全性',1360,0,1200,2200);
A.text(atlas,'Title','ログインボーナスの実行状態',48,40,1104,'Title');A.text(atlas,'Help','すべて合成例。202受付と、最終GETで確認した成功を区別します。',48,112,1104,'Body','muted');
const states=[['pending','受付済み・まだ実行していない','warning'],['running','サーバーで実行中','warning'],['retry_wait','安全なGET / 送信前処理のみ、10秒後に再確認','warning'],['success','最終GETで受取成功を確認','success'],['already_claimed','すでに受取済みとGETで確認','success'],['action_required','追加認証が必要。自動処理を停止','warning'],['unknown','送信結果が不確定。無条件POST再送を禁止','warning'],['failed','失敗。安全なコード・開始／終了時刻を表示','error'],['stopped','認証・アカウント等の条件により停止','muted']];
for(let i=0;i<states.length;i++){
 let [label,help,color]=states[i],row=A.board(atlas,'RunState / '+label,48,192+i*156,1104,132,'surface',8);A.flex(row,'column',8,16);A.flowText(row,'State',label,'Section',color);A.flowText(row,'Meaning',help,'Body','text');A.component('RunState/'+label,row);
}
A.text(atlas,'Retry safety','初回 + 最大3回の安全な再確認。ページ閲覧によらずサーバーが実行。\n追加認証は自動突破しない。unknownに「もう一度受け取る」は表示しない。',48,1656,1104,'Body','muted',96);
let skeleton=A.board(atlas,'Skeleton',48,1824,520,240,'surface',8);A.rect(skeleton,'Label placeholder',24,24,240,32,'elevated',8);A.rect(skeleton,'Value placeholder',24,88,400,64,'elevated',8);A.component('Skeleton/GameStatus',skeleton);
const previews=A.artboard('04','game-masters','ゲーム状態 / 再利用マスター',2720,0,1200,1700);
let genshin=A.game(previews,48,48,1096,false,false,false);A.component('GameStatus/Genshin',genshin);
let starrail=A.game(previews,48,560,1096,false,true,true);A.component('GameStatus/StarRail',starrail);
let panel=A.panel(previews,'Panel / sample',48,1152,520,[{type:'title',text:'共通パネル'},{type:'p',text:'見出し・本文・操作をFlexで配置'},{type:'button',label:'状態を更新'}]);A.component('Panel',panel.board);
const controls=A.artboard('04','control-states','操作部品 / Hover・Focus・Busy',4080,0,1200,920);
for(let i=0;i<3;i++){let k=['Hover','Focus','Busy'][i],instance=A.instance(controls,'Button / '+k,'Button/'+(i===2?'Disabled':'Primary'),48,80+i*200,520);let txt=penpotUtils.findShape(s=>s.type==='text',instance);txt.characters=i===2?'処理中…':'保存する';if(i===1){const ring=A.rect(controls,'Focus ring / 4px offset',44,76+i*200,528,56,'canvas',12);ring.strokes=[{strokeColor:A.colors.text,strokeWidth:2,strokeStyle:'solid',strokeAlignment:'outer'}];ring.applyToken(A.tokens.text,['strokeColor']);ring.sendToBack();}A.text(controls,'State label',k,624,88+i*200,520,'Section');}
A.text(controls,'Behavior','Hoverでレイアウトを動かさない。Focusは2px＋4px offset。\nBusyは既存のsingle-flight操作制限に対応する。',48,736,1104,'Body','muted',96);
return {system:ds.id,components:penpot.library.local.components.length};
