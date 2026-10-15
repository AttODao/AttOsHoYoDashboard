// Synthetic design preview only. No API, authenticator or real credentials.
// Only language/theme preferences use localStorage; never password visibility or input.
const app=document.querySelector('#app'),toast=document.querySelector('#toast');
const params=new URLSearchParams(location.search);
const preference=(key,allowed,fallback)=>{try{const value=params.get(key)||localStorage.getItem('agd-preview-'+key);return allowed.includes(value)?value:fallback;}catch{return allowed.includes(params.get(key))?params.get(key):fallback;}};
let language=preference('lang',['ja','en'],'ja'),theme=preference('theme',['dark','light'],'dark'),fieldNo=0;
const savePreference=(key,value)=>{try{localStorage.setItem('agd-preview-'+key,value);}catch{/* Session-only when browser storage is unavailable. */}};
const fixture=['player','admin','provisional','recovery'].includes(params.get('fixture'))?params.get('fixture'):'player';
const recovery=fixture==='recovery',states=['normal','loading','empty','error','stale','storage','queued','unknown','passkey-cancel','logout-error'];
let state=states.includes(params.get('state'))?params.get('state'):'normal';
let view='',setupComplete=!['provisional','recovery'].includes(fixture),restricted=!setupComplete,passwordDone=false,cookieSaved=false,authMethod='password',game='genshin';
const settingsTabs=[['uids','ゲーム表示'],['bonus','ログインボーナス'],['link','HoYoLAB連携'],['security','セキュリティ'],['delete','ユーザー削除']];
let settingsTab=settingsTabs.some(([k])=>k===params.get('tab'))?params.get('tab'):'uids';
const visible={genshin:fixture!=='provisional',starrail:fixture!=='provisional'},automatic={genshin:false,starrail:false},schedule={genshin:'06:00',starrail:'06:00'};
const games={genshin:{name:'原神',uid:'800000001',resource:'天然樹脂',current:142,max:200,full:'10/14 21:44',metric:'洞天宝銭',value:'1,820',metric2:'参量物質変化器',value2:'未取得'},starrail:{name:'崩壊：スターレイル',uid:'800000002',resource:'開拓力',current:178,max:300,full:'10/15 02:12',metric:'予備開拓力',value:'640',metric2:'模擬宇宙',value2:'7,500 / 14,000'}};
const icons={eye:'M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0',hide:'M3 3l18 18M9.5 5.3A12 12 0 0 1 12 5c6.5 0 10 7 10 7a18 18 0 0 1-4 4.7M6 6.5A19 19 0 0 0 2 12s3.5 7 10 7a12 12 0 0 0 5-1.1M10 10a3 3 0 0 0 4 4',close:'M6 6l12 12M6 18 18 6',menu:'M4 6h16M4 12h16M4 18h16',settings:'M4 7h16M4 17h16M8 4v6M16 14v6',link:'M10 13a4 4 0 0 0 6 0l4-4a4 4 0 0 0-6-6l-2 2M14 11a4 4 0 0 0-6 0l-4 4a4 4 0 0 0 6 6l2-2',user:'M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M4 21v-2a8 8 0 0 1 16 0v2',logout:'M10 4H4v16h6M9 12h12m-4-4 4 4-4 4'};
const icon=name=>`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${icons[name]}"></path></svg>`;
const button=(text,action,kind='',disabled=false)=>`<button type="button" data-action="${action}" class="${kind}" ${disabled?'disabled':''}>${text}</button>`;
function field(label,value='',type='text',id=''){
 id||='field-'+fieldNo++;
 const input=`<input id="${id}" type="${type}" value="${value}" readonly aria-describedby="preview-readonly" ${type==='password'?`data-password autocomplete="${label==='パスワード'||label.startsWith('現在')||label.startsWith('管理者自身')?'current-password':'new-password'}"`:id==='username'?'autocomplete="username"':''}>`;
 if(type==='password')return `<div class="field"><label for="${id}">${label}</label><div class="password-control">${input}<button type="button" class="password-eye" data-action="reveal-${id}" aria-label="パスワードを表示" aria-controls="${id}" aria-pressed="false">${icon('eye')}</button></div></div>`;
 return `<label class="field"><span>${label}</span>${input}</label>`;
}
const notice=(title,body='',kind='')=>`<div class="notice ${kind}"><strong>${title}</strong>${body?`<p>${body}</p>`:''}</div>`;
const username=fixture==='admin'?'demo_admin':fixture==='provisional'?'demo_pending':recovery?'demo_recovery':'demo_player';
function header(){
 const menu=`<button type="button" class="menu" id="menu-trigger" data-action="menu" aria-label="メニューを開く" aria-controls="account-menu" aria-expanded="false">${icon('menu')}</button>`;
 return `<header class="topbar"><div class="topbar-inner"><span class="wordmark">AGD</span><div class="account">${view==='login'||view==='deleted'?'':`<span class="muted account-name">${username}</span>${restricted?button('ログアウト','logout'):menu}`}</div></div></header>`;
}
function intro(title,back=false){return `<div class="page-intro"><h1 tabindex="-1">${title}</h1>${back?button('ダッシュボードへ','dashboard'):''}</div>`;}
function stateNotice(){
 const messages={loading:['読み込み中'],error:['ゲーム状態を取得できませんでした','前回のデータを表示しています。','error'],stale:['前回取得 10/14 17:58','','warn'],storage:['暗号化設定を利用できません','管理者に確認してください。','error'],queued:['実行要求を受け付けました（202）','受け取り完了ではありません。'],unknown:['受け取り結果が不明です','再送せず、状態を再取得してください。','warn']};
 return messages[state]?notice(...messages[state]):'';
}
function switchControl(key){return `<label class="switch"><input type="checkbox" role="switch" aria-label="${games[key].name}の自動受け取り" data-automatic="${key}" ${automatic[key]?'checked':''} ${state==='storage'?'disabled':''}><span class="switch-track"></span><span data-switch-label="${key}">${automatic[key]?'ON':'OFF'}</span></label>`;}
function tabs(items,selected,kind,label){return `<div class="tabs ${kind}-tabs" role="tablist" aria-label="${label}">${items.map(([k,t])=>`<button type="button" role="tab" id="${kind}-tab-${k}" aria-controls="${kind}-panel-${k}" aria-label="${t}" aria-selected="${selected===k}" tabindex="${selected===k?0:-1}" data-${kind}-tab="${k}">${kind==='settings'?`<span class="tab-full">${t}</span><span class="tab-short" aria-hidden="true">${{uids:'ゲーム',bonus:'ボーナス',link:'連携',security:'認証',delete:'削除'}[k]}</span>`:t}</button>`).join('')}</div>`;}
function gameCard(key){
 const g=games[key],empty=state==='empty'||!visible[key];
 let body;
 if(empty)body=`<div class="empty"><h3>表示するUIDがありません</h3>${button('UIDを選ぶ','uids')}</div>`;
 else if(state==='loading')body=`<div class="empty" aria-busy="true"><h3>読み込み中</h3>${button('更新中','refresh','',true)}</div>`;
 else body=`<label class="uid-select"><span>表示中のUID</span><select aria-label="${g.name}の表示中のUID"><option>Asia · UID ${g.uid}</option></select></label><p class="resource-label">${g.resource}</p><div class="resource"><strong>${g.current}</strong><span>/ ${g.max}</span></div><progress value="${g.current}" max="${g.max}" aria-label="${g.resource}の回復量"></progress><div class="row"><span>全回復の目安</span><strong>${g.full}</strong></div><div class="metrics"><div class="metric"><small>${g.metric}</small><strong>${g.value}</strong></div><div class="metric"><small>${g.metric2}</small><strong class="metric-text">${g.value2}</strong></div></div><div class="expeditions"><h3>${key==='genshin'?'探索派遣':'派遣'}</h3><div class="expedition"><span>${key==='genshin'?'璃月の探索':'合成素材の回収'}</span><span class="status">完了 · 回収可能</span></div><div class="expedition"><span>${key==='genshin'?'モンドの探索':'経験素材の回収'}</span><span class="muted">進行中 · 21:20帰還</span></div></div><div class="row last-updated"><small>最終取得 10/14 17:58</small>${button('再取得','refresh')}</div>`;
 return `<section class="panel game-panel ${key}" id="game-panel-${key}" ${matchMedia('(max-width:900px)').matches?`role="tabpanel" aria-labelledby="game-tab-${key}"`:`role="region" aria-labelledby="game-title-${key}"`} data-selected="${game===key}"><h2 class="game-title" id="game-title-${key}">${g.name}</h2><div class="card-automation"><span>ログインボーナス</span>${switchControl(key)}</div>${body}</section>`;
}
function dashboard(){return intro('ダッシュボード')+stateNotice()+tabs(Object.entries(games).map(([k,g])=>[k,g.name]),game,'game','ゲーム')+`<div class="dashboard-grid">${Object.keys(games).map(gameCard).join('')}</div>`;}
function login(){return `<div class="auth-layout"><section class="panel auth-panel"><h1 tabindex="-1">ログイン</h1>${state==='passkey-cancel'?notice('認証を中断しました','','warn'):''}${state==='logout-error'?notice('ログアウトを確認できません','','error')+button('再試行','logout'):''}${field('ユーザー名',username,'text','username')}<fieldset class="auth-method"><legend>ログイン方法</legend><label><input type="radio" name="method" value="password" ${authMethod==='password'?'checked':''}>パスワード</label><label><input type="radio" name="method" value="passkey" ${authMethod==='passkey'?'checked':''}>パスキー</label></fieldset><div id="password-field" ${authMethod==='passkey'?'hidden':''}>${field('パスワード','','password')}</div><div class="auth-submit">${button(authMethod==='passkey'?'パスキーでログイン':'ログイン','sign-in','primary full',state==='loading')}</div></section></div>`;}
function setup(){
 const password=`<section class="panel"><h2>パスワードを変更</h2>${field('新しいパスワード','','password')}${field('新しいパスワード（確認）','','password')}${button(recovery?'変更してログイン':'パスワードを変更','setup-password','primary')}</section>`;
 const cookie=`<section class="panel"><h2>HoYoLAB連携</h2>${field('Cookie','','text')}<div class="actions">${button('Cookieを保存','setup-cookie','',state==='storage')}${button('本人確認を実行','setup-verify','primary',!cookieSaved||state==='storage')}</div>${cookieSaved?notice('本人確認待ち','','warn'):''}${state==='storage'?stateNotice():''}</section>`;
 return `<div class="setup-wrap">${intro(recovery?'パスワードを再設定':'初期設定')}${recovery?'':`<div class="setup-progress"><span class="${!passwordDone?'current':''}">1. パスワード変更</span><span class="${passwordDone?'current':''}">2. HoYoLAB連携</span></div>`}${passwordDone&&!recovery?cookie:password}</div>`;
}
function timePicker(k){
 const [hour,minute]=schedule[k].split(':'),options=(n,selected)=>Array.from({length:n},(_,i)=>String(i).padStart(2,'0')).map(v=>`<option ${v===selected?'selected':''}>${v}</option>`).join('');
 return `<fieldset class="time-field"><legend>自動受け取り時刻</legend><div class="time-picker"><label><span class="muted">時</span><select data-hour="${k}" aria-label="${games[k].name}の時" ${state==='storage'?'disabled':''}>${options(24,hour)}</select></label><span class="time-colon">:</span><label><span class="muted">分</span><select data-minute="${k}" aria-label="${games[k].name}の分" ${state==='storage'?'disabled':''}>${options(60,minute)}</select></label></div><small class="timezone">タイムゾーン +09:00</small></fieldset>`;
}
function settingsContent(){
 if(settingsTab==='uids')return `<h2>ゲーム表示</h2>${Object.keys(games).map(k=>`<div class="setting-row"><div><h3>${games[k].name}</h3><small>Asia · UID ${games[k].uid}</small></div><label class="check"><input type="checkbox" data-visible="${k}" ${visible[k]?'checked':''}>表示</label></div>`).join('')}${button('UIDを同期','sync')}`;
 if(settingsTab==='bonus')return `<h2>ログインボーナス</h2>${stateNotice()}<div class="form-grid">${Object.keys(games).map(k=>`<div><h3>${games[k].name}</h3>${timePicker(k)}<div class="actions">${button('保存','save-'+k,'primary',state==='storage')}${button('今すぐ実行','run-'+k,'',state==='storage'||state==='queued'||(state==='unknown'&&k==='genshin'))}</div></div>`).join('')}</div><hr class="divider"><div class="row"><h3>実行履歴 · 直近5日</h3>${button('更新','history')}</div><ul class="history"><li><div><strong>原神 · ${state==='unknown'?'結果不明':state==='queued'?'受付済み':'受取済み'}</strong><p>10/14 06:00${state==='unknown'?' · 再送せず状態を確認':state==='queued'?' · 完了は未確認':' · 試行1'}</p></div><span class="status ${state==='unknown'?'warn':state==='queued'?'off':''}">${state==='unknown'?'unknown':state==='queued'?'pending':'success'}</span></li><li><div><strong>スターレイル · 当日分受取済み</strong><p>10/14 06:00 · 試行1</p></div><span class="status">already_claimed</span></li></ul>`;
 if(settingsTab==='link')return `<h2>HoYoLAB連携</h2>${state==='storage'?stateNotice():''}<div class="row"><span>UID · 900000001</span><span class="status">本人確認済み</span></div>${field('Cookie','','text')}<div class="actions">${button('保存','save','',state==='storage')}${button('本人確認','verify','primary',state==='storage')}</div>`;
 if(settingsTab==='security')return `<h2>セキュリティ</h2><div class="form-grid"><div><h3>パスワード変更</h3>${field('現在のパスワード','','password')}${field('新しいパスワード','','password')}${field('新しいパスワード（確認）','','password')}${button('変更','password','primary')}</div><div><h3>パスキー</h3><p>この端末 · 登録 10/12</p><div class="actions">${button('登録','passkey')}${button('削除','passkey-delete','danger')}</div><hr class="divider"><h3>セッション</h3><p>有効期限 · 10/21 18:00</p></div></div>`;
 return `<h2>ユーザー削除</h2><p>${username}</p>${field('ユーザー名を完全一致で入力',username)}${field('現在のパスワード','','password')}${button('ユーザーを削除','delete-self','danger')}`;
}
function settings(){return intro('設定',true)+tabs(settingsTabs,settingsTab,'settings','設定項目')+`<section class="panel settings-section" role="tabpanel" id="settings-panel-${settingsTab}" aria-labelledby="settings-tab-${settingsTab}">${settingsContent()}</section>`;}
function admin(){
 if(fixture!=='admin')return intro('アクセスできません',true)+notice('管理者権限が必要です','','error');
 return intro('管理者設定',true)+`<div class="stack"><section class="panel"><h2>ユーザー作成</h2>${field('ユーザー名','demo_new','text','new-username')}${button('ユーザーを作成','admin-create','primary')}</section><section class="panel"><h2>ユーザー管理</h2>${['demo_player','demo_inactive'].map((name,i)=>`<div class="setting-row"><div><h3>${name}</h3><span class="status ${i?'off':''}">${i?'無効':'有効'}</span></div><div class="actions">${button('パスワード再発行','admin-reset-'+i)}${button(i?'再有効化':'無効化',i?'admin-enable':'admin-disable')}${button('ユーザー削除','admin-delete-'+i,'danger')}</div></div>`).join('')}</section><section class="panel" id="admin-logs"><h2>ログ</h2><span class="status off">取得API未提供</span>${button('読み込む','admin-logs','',true)}</section></div>`;
}
function accepted(adminTarget=false){return intro('ユーザー削除を受け付けました')+`<section class="panel">${notice('削除処理待ち','', 'success')}${button(adminTarget?'ユーザー管理へ戻る':'ログイン画面へ',adminTarget?'admin':'logout','primary')}</section>`;}
function reviewTools(){return `<footer class="review-tools"><strong>デザイン確認用 / 製品外</strong><div class="row"><a href="#dashboard">Dashboard</a><a href="#login">ログイン</a><a href="?fixture=provisional#setup">初期登録</a><a href="?fixture=recovery#setup">再発行後</a><a href="?fixture=admin#admin">管理者</a></div><label>状態 <select id="preview-state">${states.map(s=>`<option ${state===s?'selected':''}>${s}</option>`).join('')}</select></label></footer>`;}
function render(next,focus=false){
 if(!['dashboard','login','setup','settings','admin','deleted','admin-deleted'].includes(next))next='dashboard';
 if(restricted&&next!=='login')next='setup';view=next;toast.hidden=true;
 const content={dashboard,login,setup,settings,admin,deleted:()=>accepted(),'admin-deleted':()=>accepted(true)};
 fieldNo=0;document.documentElement.style.overflow='';document.documentElement.lang=language;document.documentElement.dataset.theme=theme;
 app.innerHTML=header()+`<main class="container">${content[view]()}</main>`+reviewTools();localize(app);
 document.title=`AGD — ${document.querySelector('h1,h2')?.textContent}${language==='en'?' (Design draft)':'（デザイン案）'}`;
 if(location.hash!=='#'+view)history.replaceState(null,'','#'+view);if(focus)document.querySelector('h1')?.focus({preventScroll:true});
}
function notify(text){toast.textContent=translate(text);toast.hidden=false;}
function navigate(target){render(target,true);window.scrollTo({top:0,behavior:'instant'});}
function section(id){settingsTab=id;navigate('settings');}
function openDialog(title,content){document.querySelector('#confirmation')?.remove();const d=document.createElement('dialog');d.id='confirmation';d.setAttribute('aria-labelledby','confirmation-title');d.innerHTML=`<h2 id="confirmation-title">${title}</h2>${content}`;app.appendChild(d);localize(d);d.showModal();}
function maskPasswords(){for(const input of document.querySelectorAll('[data-password]')){input.type='password';const b=input.parentElement.querySelector('.password-eye');b.setAttribute('aria-pressed','false');b.setAttribute('aria-label',translate('パスワードを表示'));b.innerHTML=icon('eye');}}
function closeMenu(){document.querySelector('#account-menu')?.close();document.documentElement.style.overflow='';const trigger=document.querySelector('#menu-trigger');trigger?.setAttribute('aria-expanded','false');trigger?.focus({preventScroll:true});}
function openMenu(){
 maskPasswords();document.querySelector('#account-menu')?.remove();
 const d=document.createElement('dialog');d.id='account-menu';d.className='menu-drawer';d.setAttribute('aria-label',translate('アカウントメニュー'));
 d.innerHTML=`<div class="drawer-heading"><h2>メニュー</h2><button type="button" class="icon-button" data-action="close-menu" aria-label="メニューを閉じる">${icon('close')}</button></div><nav aria-label="アカウントメニュー">${button(icon('link')+'HoYoLAB連携','link')}${button(icon('settings')+'設定','settings')}${fixture==='admin'?button(icon('user')+'管理者設定','admin'):''}${button(icon('logout')+'ログアウト','logout')}</nav><div class="menu-preferences"><button type="button" class="mode-switch" role="switch" aria-checked="${theme==='dark'}" data-action="toggle-theme"><span>ダークモード</span><span class="mode-track"><span class="mode-thumb"></span></span></button><fieldset class="language-options"><legend>言語</legend><div>${['ja','en'].map(lang=>`<button type="button" data-action="language-${lang}" lang="${lang}" aria-pressed="${language===lang}">${lang==='ja'?'日本語':'English'}</button>`).join('')}</div></fieldset></div>`;
 app.appendChild(d);localize(d);d.addEventListener('cancel',e=>{e.preventDefault();closeMenu();});d.addEventListener('click',e=>{const r=d.getBoundingClientRect();if(e.target===d&&(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom))closeMenu();});
 d.showModal();document.documentElement.style.overflow='hidden';document.querySelector('#menu-trigger').setAttribute('aria-expanded','true');
}
function generatedPassword(title,name){openDialog(title,`<p>${name}</p><label class="field"><span>仮パスワード</span><output class="generated-password">DEMO-ONLY-4829</output></label><div class="actions">${button('コピー','copy-generated')}${button('閉じる','cancel-delete','primary')}</div>`);}
function activateTab(kind,key){if(kind==='settings')settingsTab=key;else game=key;render(view);document.querySelector(`#${kind}-tab-${key}`)?.focus({preventScroll:true});}
app.addEventListener('click',e=>{
 const tab=e.target.closest('[role=tab]');if(tab){activateTab(tab.dataset.settingsTab?'settings':'game',tab.dataset.settingsTab||tab.dataset.gameTab);return;}
 const b=e.target.closest('[data-action]');if(!b||b.disabled)return;const action=b.dataset.action;
 if(action==='menu'){openMenu();return;}if(action==='close-menu'){closeMenu();return;}
 if(action.startsWith('language-')||action==='toggle-theme'){
  const focusAction=action;if(action==='toggle-theme'){theme=theme==='dark'?'light':'dark';savePreference('theme',theme);}else{language=action.slice(9);savePreference('lang',language);}
  render(view);openMenu();document.querySelector(`[data-action="${focusAction}"]`)?.focus({preventScroll:true});return;
 }
 if(action.startsWith('reveal-')){const input=document.getElementById(b.getAttribute('aria-controls')),shown=input.type==='password';input.type=shown?'text':'password';b.setAttribute('aria-pressed',String(shown));b.setAttribute('aria-label',translate(shown?'パスワードを隠す':'パスワードを表示'));b.innerHTML=icon(shown?'hide':'eye');return;}
 if(restricted&&!['logout','setup-password','setup-cookie','setup-verify','sign-in'].includes(action))return;
 if(action==='logout'){if(state==='logout-error'){notify('ログアウトできませんでした');return;}restricted=false;navigate('login');return;}
 if(action==='sign-in'){restricted=!setupComplete;navigate(restricted?'setup':'dashboard');return;}
 if(action==='setup-password'){passwordDone=true;if(recovery){setupComplete=true;restricted=false;navigate('dashboard');}else render('setup');return;}
 if(action==='setup-cookie'){cookieSaved=true;render('setup');return;}
 if(action==='setup-verify'){if(!passwordDone||!cookieSaved||recovery||state==='storage')return;setupComplete=true;restricted=false;state='normal';navigate('dashboard');return;}
 if(action==='link'){section('link');return;}if(action==='uids'){section('uids');return;}
 if(['dashboard','settings','admin'].includes(action)){navigate(action);return;}
 if(action.startsWith('save-')){const k=action.slice(5),hour=document.querySelector(`[data-hour=${k}]`),minute=document.querySelector(`[data-minute=${k}]`);schedule[k]=hour.value+':'+minute.value;notify('保存しました');return;}
 if(action.startsWith('run-')){state='queued';section('bonus');return;}
 if(action==='delete-self'){navigate('deleted');return;}
 if(action.startsWith('admin-delete-')){const name=action.endsWith('1')?'demo_inactive':'demo_player';openDialog(`${name} を削除`,`${field('対象名を完全一致で入力',name)}${field('管理者自身の現在のパスワード','','password')}<div class="actions">${button('キャンセル','cancel-delete')}${button('ユーザーを削除','accept-admin-delete','danger')}</div>`);return;}
 if(action.startsWith('admin-reset-')){const name=action.endsWith('1')?'demo_inactive':'demo_player';openDialog(`${name} のパスワード再発行`,`<div class="actions">${button('キャンセル','cancel-delete')}${button('再発行','accept-admin-reset-'+name,'primary')}</div>`);return;}
 if(action.startsWith('accept-admin-reset-')){generatedPassword('パスワードを再発行しました',action.slice('accept-admin-reset-'.length));return;}
 if(action==='admin-create'){generatedPassword('ユーザーを作成しました','demo_new');return;}
 if(action==='cancel-delete'){document.querySelector('#confirmation').close();return;}
 if(action==='accept-admin-delete'){navigate('admin-deleted');return;}
 const messages={save:'保存しました',password:'パスワードを変更しました',passkey:'パスキーを待機中','passkey-delete':'削除しました',verify:'本人確認済み',sync:'UIDを同期しました',refresh:'更新しました',history:'更新しました','admin-disable':'無効化しました','admin-enable':'再有効化しました','copy-generated':'コピーの表示例'};
 if(messages[action])notify(messages[action]);
});
app.addEventListener('keydown',e=>{const tab=e.target.closest('[role=tab]');if(!tab||!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const list=[...tab.parentElement.querySelectorAll('[role=tab]')],i=list.indexOf(tab),n=e.key==='Home'?0:e.key==='End'?list.length-1:(i+(e.key==='ArrowRight'?1:-1)+list.length)%list.length;list[n].click();});
app.addEventListener('change',e=>{
 if(e.target.name==='method'){authMethod=e.target.value;render('login');document.querySelector(`input[name=method][value=${authMethod}]`)?.focus();}
 if(e.target.dataset.automatic){const k=e.target.dataset.automatic;automatic[k]=e.target.checked;document.querySelector(`[data-switch-label=${k}]`).textContent=automatic[k]?'ON':'OFF';}
 if(e.target.dataset.visible)visible[e.target.dataset.visible]=e.target.checked;
 if(e.target.id==='preview-state'){state=e.target.value;render(view);document.querySelector('#preview-state')?.focus();}
});
document.addEventListener('keydown',e=>{if(e.key==='Escape')toast.hidden=true;});
window.addEventListener('blur',maskPasswords);
document.addEventListener('visibilitychange',()=>{if(document.hidden)maskPasswords();});
window.addEventListener('hashchange',()=>render(location.hash.slice(1),true));
matchMedia('(max-width:900px)').addEventListener('change',()=>{if(view==='dashboard')render(view);});
const initial=location.hash.slice(1)||'dashboard';if(initial==='setup'){setupComplete=false;restricted=true;if(!recovery){visible.genshin=false;visible.starrail=false;}}
render(initial);
