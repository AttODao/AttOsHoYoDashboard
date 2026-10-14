const A=storage.attos;
if(!A)throw new Error('Run setup first');
A.navLabels=['ダッシュボード','HoYoLAB連携','UID表示設定','ログインボーナス','セキュリティ','ユーザー管理','アカウント削除'];
A.navKeys=['dashboard','link','uids','rewards','security','admin','delete'];
A.shell=(b,title,active,mobile=false,admin=false)=>{
 const w=b.width;
 if(mobile){
  let head=A.board(b,'Header',0,0,w,80,'rail');A.text(head,'Brand','AttOs',20,16,180,'Section');A.button(head,'メニュー',w-128,16,108,'Secondary','menu-m');
  A.text(b,'Page title',title,20,112,w-40,'Title');
  A.text(b,'Synthetic data label','デザイン案・合成データ',20,172,w-40,'Small','muted');
  return {x:20,y:220,w:w-40};
 }
 let rail=A.board(b,'Navigation / desktop',0,0,232,b.height,'rail');
 A.text(rail,'Brand','AttOs',28,32,176,'Display');A.text(rail,'Product','Game Dashboard',28,108,176,'Small');
 for(let i=0;i<A.navLabels.length;i++){
  if(!admin&&i===5)continue;
  let n=A.board(rail,'Nav / '+A.navKeys[i],16,184+i*60,200,48,i===active?'primary':'rail',8);A.text(n,'Label',A.navLabels[i],12,10,176,'Label',i===active?'onPrimary':'text');A.links.push({shape:n,target:A.navKeys[i]+'-d'});
 }
 A.text(rail,'Account',admin?'demo_admin\n管理者':'demo_player\n一般ユーザー',28,b.height-200,176,'Body','text',64);
 A.text(rail,'Server metadata','固定 +09:00\nセッション 7日',28,b.height-112,176,'Small','text',56);
 A.text(b,'Page title',title,288,40,720,'Title');A.text(b,'Synthetic data label','デザイン案・合成データ',288,104,600,'Small','muted');A.button(b,'ログアウト',1216,40,168,'Secondary','login-d');
 return {x:288,y:160,w:1096};
};
A.divider=(p,y,w)=>A.rect(p,'Divider',0,y,w,1,'line');
A.selector=(p,x,y,w,text)=>{let b=A.board(p,'UID selector',x,y,w,56,'elevated',8);b.strokes=[{strokeColor:A.colors.line,strokeWidth:1,strokeStyle:'solid',strokeAlignment:'inner'}];b.applyToken(A.tokens.line,['strokeColor']);A.text(b,'Selected UID',text,16,14,w-64,'Label');let g=penpot.createShapeFromSvg('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><path d="m6 9 6 6 6-6" fill="none" stroke="#F0F4FC" stroke-width="2"/></svg>');if(g){b.appendChild(g);g.x=b.x+w-40;g.y=b.y+16;const path=penpotUtils.findShape(s=>s.type==='path',g);if(path)path.applyToken(A.tokens.text,['strokeColor']);}return b;};
A.game=(p,x,y,w,mobile=false,star=false,stale=false)=>{
 let h=mobile?740:464,b=A.board(p,'GameStatus / '+(star?'StarRail':'Genshin'),x,y,w,h,star?'starrailSurface':'genshinSurface',12),accent=star?'starrail':'genshin';
 let pad=mobile?20:32,inner=w-pad*2;
 A.text(b,'Game title',star?'崩壊：スターレイル':'原神',pad,pad,inner,'Title',accent);
 A.text(b,'Game identity',star?'HONKAI: STAR RAIL':'GENSHIN IMPACT',pad,mobile?84:88,inner,'Label',accent);
 A.text(b,'Account metadata',star?'UID 900000002 · Asia':'UID 800000001 · Asia',pad,mobile?128:132,inner,'Body','text');
 A.text(b,'Fetched',stale?'最終取得 10/14 17:45 · 古いデータ':'最終取得 10/14 18:05 · 最新',pad,mobile?166:168,inner,'Small',stale?'warning':'text');
 if(stale)A.text(b,'Sanitized error','更新失敗 · upstream_unavailable\nエラー時刻 10/14 18:05',pad,208,inner,'Small','warning',56);
 let yy=stale?288:232;
 A.text(b,'Primary metric label',star?'開拓力':'天然樹脂',pad,yy,inner,'Label',accent);
 A.text(b,'Primary metric value',star?'178 / 300':'142 / 200',pad,yy+32,mobile?inner:380,'Number');
 A.rect(b,'Resource track',pad,yy+120,mobile?inner:Math.min(400,w*0.38),8,'line',4);
 A.rect(b,'Resource value',pad,yy+120,(mobile?inner:Math.min(400,w*0.38))*(star?178/300:142/200),8,accent,4);
 A.text(b,'Full at',star?'全回復 10/15 01:41':'全回復 10/15 01:49',pad,yy+144,mobile?inner:440,'Small','text');
 let rx=mobile?pad:Math.round(w*.53),ry=mobile?yy+216:232,rw=mobile?inner:w-rx-pad;
 const entries=star?[['予備開拓力','未取得'],['デイリー訓練','300 / 500']]:[['洞天宝銭','1,200 / 2,400'],['宝銭満杯','10/15 14:05'],['デイリー依頼','3 / 4'],['追加報酬','未受取']];
 for(let i=0;i<entries.length;i++){let yy2=ry+i*44;A.text(b,'Metric label / '+entries[i][0],entries[i][0],rx,yy2,rw*.52,'Small',accent);A.text(b,'Metric value / '+entries[i][0],entries[i][1],rx+rw*.52,yy2,rw*.48,'Label');}
 A.button(b,'状態を再取得',mobile?pad:w-224,mobile?h-72:32,mobile?inner:192,'Secondary');
 return b;
};
A.accountList=(p,x,y,w,mobile=false)=>{let b=A.board(p,'Visible detected UIDs',x,y,w,260,'surface',8);A.text(b,'Heading','表示中のゲームUID',24,20,w-48,'Section');A.text(b,'Rule','選択したUIDのみ状態を取得',24,64,w-48,'Small','muted');A.text(b,'Genshin','原神\n800000001 · Asia',24,108,w-48,'Body','text',64);A.text(b,'StarRail','崩壊：スターレイル\n900000002 · Asia',24,180,w-48,'Body','text',64);return b;};
A.bonus=(p,x,y,w,mobile=false)=>{let b=A.board(p,'Reward summary',x,y,w,264,'surface',8);A.text(b,'Heading','ログインボーナス',24,20,w-48,'Section');A.text(b,'Scope','HoYoLABアカウント単位・UID選択とは独立',24,64,w-48,'Small','muted');A.text(b,'Genshin','原神  未受取 · 自動OFF',24,112,w-48,'Label','genshin');A.text(b,'StarRail','スターレイル  受取済み · 自動OFF',24,156,w-48,'Label','starrail');A.text(b,'Checked','確認 10/14 18:05',24,204,w-48,'Small','muted');return b;};
A.dashboard=(b,mobile=false,star=false,stale=false,context)=>{
 let c=context||A.shell(b,'ダッシュボード',0,mobile),yy=c.y;
 let s=A.selector(b,c.x,yy,c.w,mobile?(star?'スターレイル 900000002':'原神 800000001'):(star?'崩壊：スターレイル / 900000002 / Asia':'原神 / 800000001 / Asia'));
 A.links.push({shape:s,target:star?'dashboard-'+(mobile?'m':'d'):'starrail-'+(mobile?'m':'d')});
 A.game(b,c.x,yy+80,c.w,mobile,star,stale);
 let below=yy+80+(mobile?740:464)+24;
 if(mobile){A.accountList(b,c.x,below,c.w,true);A.bonus(b,c.x,below+284,c.w,true);A.button(b,'UID表示を設定',c.x,below+572,c.w,'Secondary','uids-m');A.button(b,'ボーナス設定と履歴',c.x,below+636,c.w,'Secondary','rewards-m');}
 else{A.accountList(b,c.x,below,400);A.bonus(b,c.x+424,below,672);A.button(b,'UID表示を設定',c.x,below+288,400,'Secondary','uids-d');A.button(b,'ボーナス設定と履歴',c.x+424,below+288,672,'Secondary','rewards-d');}
 const ey=below+(mobile?716:360);A.text(b,'Expedition heading',star?'派遣':'探索派遣',c.x,ey,c.w,'Section');A.text(b,'Expedition metadata',star?'合成派遣B · 探索中 · 残り 02:10:00':'合成派遣A · 探索中 · 残り 01:30:00',c.x,ey+44,c.w,'Body','muted');if(ey+120>b.height)b.resize(b.width,ey+144);
};
await penpot.openPage(A.pages['01']);
const a=A.artboard('01','concept-a','A / Orbit Folio / 推奨候補・未承認',0,0,1440,1180);A.dashboard(a);
const b=A.artboard('01','concept-b','B / Arcade Index / 比較案・未承認',1536,0,1440,1180,'alternate');
A.board(b,'Header',0,0,1440,96,'starrailSurface');A.text(b,'Brand','AttOs',40,24,220,'Display','alternateAccent');A.text(b,'Product','Game Dashboard',260,40,380,'Label');A.text(b,'User','demo_player · 合成データ',1080,40,320,'Small');
for(let i=0;i<5;i++){let n=A.board(b,'Nav / '+A.navKeys[i],40+i*268,120,252,48,i===0?'alternateAccent':'alternate',4);A.text(n,'Label',A.navLabels[i],16,12,220,'Label',i===0?'onPrimary':'text');}
A.text(b,'Title','ダッシュボード',40,208,920,'Title');A.text(b,'Draft label','比較デザイン案・未承認',1040,224,340,'Small','muted');
A.accountList(b,40,296,304);A.button(b,'UID表示を設定',40,572,304,'Secondary');
let sel=A.selector(b,376,296,636,'原神 / 800000001 / Asia');
A.game(b,376,376,636,false,false,false);
A.bonus(b,1044,296,356);A.button(b,'ボーナス設定と履歴',1044,624,356,'Secondary');
A.text(b,'Expeditions','探索派遣',376,888,636,'Section');A.text(b,'Expedition row','合成派遣A · 探索中 · 残り 01:30:00',376,936,636,'Body','muted');
A.text(b,'Concept tradeoff','濃いプラムとアプリコット。3カラムの高密度表示。\n長い日本語ラベルとスマートフォンへの展開には注意。',40,1056,1360,'Body','muted',64);
return {concepts:[{id:a.id,name:a.name},{id:b.id,name:b.name}]};
