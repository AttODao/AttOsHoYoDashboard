const A=storage.attos;
const titles={loading:'読み込み中',empty:'表示対象のUIDなし',error:'状態の取得エラー','permission':'権限がありません','passkey-prompt':'パスキーの確認','passkey-cancel':'パスキーを中断','reward-saved':'設定を保存しました','reward-queued':'実行を受け付けました','password-saved':'パスワードを変更しました','passkey-deleted':'パスキーを削除しました','admin-disabled':'ユーザーを無効化しました','admin-reenabled':'ユーザーを再有効化しました','admin-reset':'復旧用パスワードを設定しました','admin-deletion-accepted':'ユーザー削除を受け付けました','deletion-accepted':'削除要求を受け付けました','logout-error':'ログアウトを確認できません','storage':'連携設定を利用できません','uids-saved':'表示設定を保存しました'};
A.state=(key,m)=>{
 const suffix=m?'m':'d',title=titles[key],v=A.make(key,title,m,m?1120:1040,key.startsWith('admin')?5:key.startsWith('reward')?3:key.startsWith('passkey')||key.startsWith('password')?4:key==='storage'?1:key==='uids-saved'?2:0),{b,c}=v;
 const small=(text,color='muted')=>({type:'small',text,color}),text=(text,color)=>({type:'p',text,color}),button=(label,kind,target)=>({type:'button',label,kind,target}),notice=(title,body,kind='warning')=>({type:'notice',title,body,kind,height:144});
 let items=[];
 if(key==='loading'){
  let z=A.panel(b,'Loading feedback',c.x,c.y,c.w,[{type:'title',text:'選択中の原神を読み込み中'},small('UID 800000001 · Asia\n取得完了まで操作はできません。'),button('取得中…','Disabled')]);
  const sk=A.board(b,'Skeleton / daily status',c.x,c.y+z.height+24,c.w,224,'genshinSurface',12);A.rect(sk,'Metric placeholder',24,24,m?240:440,48,'elevated',8);A.rect(sk,'Value placeholder',24,104,m?280:640,32,'elevated',8);A.rect(sk,'Secondary placeholder',24,160,m?240:440,24,'elevated',8);return;
 }
 if(key==='empty')items=[{type:'title',text:'表示できるゲームUIDがありません'},text('本人確認は完了しています。新しいUIDは初期状態では非表示です。'),small('表示ONかつ検出済みのUIDだけが選択肢に入ります。'),button('UID表示を設定','Primary','uids-'+suffix)];
 if(key==='error')items=[notice('更新できませんでした','以前取得した状態を表示しています。再取得を試すか、Cookieの状態を確認してください。','error'),text('最終取得 10/14 17:45 · 古いデータ'),small('エラー時刻 10/14 18:05\n安全なコード upstream_unavailable'),button('状態を再取得','Primary','dashboard-'+suffix),button('HoYoLAB連携を確認','Secondary','link-'+suffix)];
 if(key==='permission')items=[notice('この操作は管理者専用です','一般ユーザーにはユーザー管理を表示しません。権限エラーが返った場合は自身の画面へ戻れます。','error'),button('ダッシュボードへ戻る','Primary','dashboard-'+suffix)];
 if(key==='passkey-prompt')items=[{type:'title',text:'ブラウザでパスキーを確認してください'},text('確認画面はブラウザ・OSが表示します。Dashboard側では完了を待ちます。'),button('確認待ち…','Disabled'),small('このボードはアプリ側の待機表示です。OSの認証画面は模倣しません。'),button('中断時の表示を見る','Secondary','passkey-cancel-'+suffix)];
 if(key==='passkey-cancel')items=[notice('パスキーの確認を中断しました','認証は完了していません。もう一度試すかパスワードでログインしてください。','error'),button('もう一度試す','Primary','passkey-prompt-'+suffix),button('パスワードでログイン','Secondary','login-'+suffix)];
 if(key==='reward-saved')items=[notice('設定を保存しました','自動受け取りはOFFです。ONや時刻を変更した場合は、次の未来の実行時刻から適用されます。','success'),small('固定時差 +09:00 · 時刻 06:00'),button('設定と履歴へ戻る','Primary','rewards-'+suffix)];
 if(key==='reward-queued')items=[notice('実行を受け付けました','202受付は受取成功ではありません。サーバーで処理し、最終確認後に結果が決まります。'),text('原神 · 手動 · pending\n作成 10/14 18:06:00'),button('履歴を更新','Primary','rewards-'+suffix),button('実行中…','Disabled')];
 if(key==='password-saved')items=[notice('パスワードを変更しました','他のセッションは失効しました。現在のセッションの期限は延長されません。','success'),button('セキュリティへ戻る','Primary','security-'+suffix)];
 if(key==='passkey-deleted')items=[notice('パスキーを削除しました','パスワードログインは引き続き利用できます。','success'),text('登録済みのパスキーはありません。'),button('セキュリティへ戻る','Primary','security-'+suffix)];
 if(key==='admin-disabled')items=[notice('対象ユーザーを無効化しました','アクセス・新しいジョブを停止し、セッションを失効させました。','success'),button('ユーザー一覧へ戻る','Primary','admin-'+suffix)];
 if(key==='admin-reenabled')items=[notice('対象ユーザーを再有効化しました','旧セッションは復活しません。自動受け取りはOFFです。','success'),button('ユーザー一覧へ戻る','Primary','admin-'+suffix)];
 if(key==='admin-reset')items=[notice('復旧用仮パスワードを設定しました','対象ユーザーのセッションとパスキーを失効させました。次回ログイン時にパスワード変更が必要です。','success'),button('ユーザー一覧へ戻る','Primary','admin-'+suffix)];
 if(key==='admin-deletion-accepted')items=[notice('対象ユーザーの削除を受け付けました','対象を無効化し、対象のセッション・パスキーを失効させました。背景でデータを削除します。','success'),small('管理者自身のセッションは継続します。受付は削除完了ではありません。'),button('ユーザー一覧へ戻る','Primary','admin-'+suffix)];
 if(key==='deletion-accepted')items=[notice('削除要求を受け付けました','対象ユーザーは無効化されました。関連データは背景で削除されます。受付はデータ削除完了ではありません。','success'),small('自己削除後はログイン画面へ戻ります。削除完了の進捗APIはありません。'),button('ログイン画面へ','Primary','login-'+suffix)];
 if(key==='logout-error')items=[notice('サーバー側のログアウトを確認できません','この画面への移動だけでは失効完了ではありません。再試行してください。','error'),button('ログアウトを再試行','Primary','login-'+suffix),button('状態を再読み込み','Secondary','login-'+suffix)];
 if(key==='storage')items=[notice('サーバーのCookie保存設定が必要です','verification_availableがfalseのときに表示します。管理者へ設定確認を依頼してください。'),button('Cookieを保存','Disabled'),button('本人確認を実行','Disabled'),button('設定状態を更新','Secondary','pending-'+suffix),button('セキュリティへ','Secondary','security-'+suffix)];
 if(key==='uids-saved')items=[notice('表示設定を保存しました','検出済みのUIDはダッシュボードで選択できます。未検出UIDは取得しません。','success'),button('ダッシュボードへ','Primary','dashboard-'+suffix)];
 let z=A.panel(b,'State / '+key,c.x,c.y,c.w,items);A.finishHeight(v,c.y+z.height+48);
 // A success after self-deletion cannot show an authenticated shell.
 if(key==='deletion-accepted'||key==='logout-error'||key.startsWith('passkey')){
  const shell=penpotUtils.findShape(s=>s.name===(m?'Header':'Navigation / desktop'),b);if(shell)shell.hidden=true;
  const logout=penpotUtils.findShape(s=>s.name==='Button / ログアウト',b);if(logout)logout.hidden=true;
 }
};
A.menu=m=>{let v=A.make('menu','メニュー',m,m?1260:1120,0),{b,c}=v;let items=[];for(let i=0;i<A.navLabels.length;i++){if(i===5)continue;items.push({type:'button',label:A.navLabels[i],target:A.navKeys[i]+'-'+(m?'m':'d')});}items.push({type:'p',text:'demo_player · 一般ユーザー'},{type:'small',text:'固定 +09:00 · セッション7日'},{type:'button',label:'ログアウト',target:'login-'+(m?'m':'d')});let z=A.panel(b,'Navigation menu',c.x,c.y,c.w,items);A.finishHeight(v,c.y+z.height);};
return 'State constructors loaded';
