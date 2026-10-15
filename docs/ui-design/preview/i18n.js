// Design-preview copy only; not production localization or credential storage.
const english={
 'ログイン':'Sign in','ログアウト':'Sign out','ユーザー名':'Username','ログイン方法':'Sign-in method','パスワード':'Password','パスキー':'Passkey','パスキーでログイン':'Sign in with a passkey',
 'メニューを開く':'Open menu','メニューを閉じる':'Close menu','アカウントメニュー':'Account menu','メニュー':'Menu','言語':'Language','表示モード':'Appearance','ダークモード':'Dark mode','日本語':'日本語','英語':'English','ダーク':'Dark','ライト':'Light',
 'パスワードを表示':'Show password','パスワードを隠す':'Hide password','新しいパスワード':'New password','新しいパスワード（確認）':'Confirm new password','現在のパスワード':'Current password','管理者自身の現在のパスワード':'Your current password',
 'ダッシュボード':'Dashboard','ダッシュボードへ':'Dashboard','設定':'Settings','設定項目':'Settings sections','管理者設定':'Administration','HoYoLAB連携':'HoYoLAB link','ゲーム表示':'Game visibility','ログインボーナス':'Daily check-in','セキュリティ':'Security','ユーザー削除':'Delete user',
 'ゲーム':'Games','ボーナス':'Check-in','連携':'Link','認証':'Security','削除':'Delete','原神':'Genshin Impact','崩壊：スターレイル':'Honkai: Star Rail','スターレイル':'Star Rail',
 '表示中のUID':'Displayed UID','表示するUIDがありません':'No UID selected','UIDを選ぶ':'Select a UID','UIDを同期':'Sync UIDs','表示':'Visible','天然樹脂':'Original Resin','開拓力':'Trailblaze Power','予備開拓力':'Reserved Power','洞天宝銭':'Realm Currency','参量物質変化器':'Parametric Transformer','未取得':'No data','模擬宇宙':'Simulated Universe','全回復の目安':'Fully replenished at',
 '探索派遣':'Expeditions','派遣':'Assignments','璃月の探索':'Liyue expedition','モンドの探索':'Mondstadt expedition','合成素材の回収':'Synthesis materials','経験素材の回収':'EXP materials','完了 · 回収可能':'Complete · Ready to claim','進行中 · 21:20帰還':'In progress · Returns 21:20','最終取得 10/14 17:58':'Updated Oct 14, 17:58','再取得':'Refresh',
 '初期設定':'Initial setup','パスワードを再設定':'Reset password','パスワードを変更':'Change password','変更してログイン':'Change and sign in','1. パスワード変更':'1. Change password','2. HoYoLAB連携':'2. Link HoYoLAB','Cookieを保存':'Save Cookie','本人確認を実行':'Verify identity','本人確認待ち':'Verification pending','本人確認':'Verify identity','本人確認済み':'Verified',
 '自動受け取り時刻':'Automatic check-in time','時':'Hour','分':'Minute','タイムゾーン +09:00':'Time zone +09:00','保存':'Save','今すぐ実行':'Run now','更新':'Refresh','実行履歴 · 直近5日':'History · Last 5 days','原神 · 受取済み':'Genshin Impact · Claimed','原神 · 結果不明':'Genshin Impact · Result unknown','原神 · 受付済み':'Genshin Impact · Queued','スターレイル · 当日分受取済み':'Star Rail · Already claimed today','10/14 06:00 · 試行1':'Oct 14, 06:00 · Attempt 1','10/14 06:00 · 再送せず状態を確認':'Oct 14, 06:00 · Check status; do not retry','10/14 06:00 · 完了は未確認':'Oct 14, 06:00 · Completion unconfirmed',
 'パスワード変更':'Change password','変更':'Change','登録':'Register','この端末 · 登録 10/12':'This device · Registered Oct 12','セッション':'Session','有効期限 · 10/21 18:00':'Expires Oct 21, 18:00','ユーザー名を完全一致で入力':'Enter the exact username','対象名を完全一致で入力':'Enter the exact target username','ユーザーを削除':'Delete user',
 'ユーザー作成':'Create user','ユーザーを作成':'Create user','ユーザー管理':'Manage users','パスワード再発行':'Reissue password','無効':'Disabled','有効':'Active','無効化':'Disable','再有効化':'Re-enable','ログ':'Logs','取得API未提供':'API unavailable','読み込む':'Load','仮パスワード':'Temporary password','コピー':'Copy','閉じる':'Close','キャンセル':'Cancel','再発行':'Reissue','パスワードを再発行しました':'Password reissued','ユーザーを作成しました':'User created','ユーザー削除を受け付けました':'User deletion accepted','削除処理待ち':'Deletion pending','ユーザー管理へ戻る':'Back to users','ログイン画面へ':'Back to sign in',
 '読み込み中':'Loading','更新中':'Refreshing','ゲーム状態を取得できませんでした':'Could not load game status','前回のデータを表示しています。':'Showing previously loaded data.','前回取得 10/14 17:58':'Last updated Oct 14, 17:58','暗号化設定を利用できません':'Encryption configuration unavailable','管理者に確認してください。':'Contact your administrator.','実行要求を受け付けました（202）':'Request accepted (202)','受け取り完了ではありません。':'Check-in is not yet confirmed.','受け取り結果が不明です':'Check-in result unknown','再送せず、状態を再取得してください。':'Refresh status; do not resend.','認証を中断しました':'Authentication cancelled','ログアウトを確認できません':'Could not confirm sign-out','ログアウトできませんでした':'Could not sign out','再試行':'Retry','アクセスできません':'Access denied','管理者権限が必要です':'Administrator access required',
 '保存しました':'Saved','パスワードを変更しました':'Password changed','パスキーを待機中':'Waiting for a passkey','削除しました':'Deleted','UIDを同期しました':'UIDs synced','更新しました':'Refreshed','無効化しました':'Disabled','再有効化しました':'Re-enabled','コピーの表示例':'Copy preview'
};
function translate(text){
 if(language!=='en')return text;
 if(english[text])return english[text];
 for(const [suffix,end]of [['の自動受け取り',' automatic check-in'],['の表示中のUID',' displayed UID'],['の時',' hour'],['の分',' minute']])if(text.endsWith(suffix)){const name=text.slice(0,-suffix.length);return (english[name]||name)+end;}
 if(text.endsWith(' を削除'))return 'Delete '+text.slice(0,-4);
 if(text.endsWith(' のパスワード再発行'))return 'Reissue password for '+text.split(' ')[0];
 return text;
}
function localize(root){
 if(language!=='en')return;
 const walk=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let n;
 while((n=walk.nextNode())){if(n.parentElement.closest('.review-tools,script,style'))continue;const s=n.textContent.trim();if(s)n.textContent=n.textContent.replace(s,translate(s));}
 for(const e of root.querySelectorAll('[aria-label]'))e.setAttribute('aria-label',translate(e.getAttribute('aria-label')));
}
