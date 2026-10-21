// Compiled-Dioxus UI check only. Synthetic replies on implemented API paths are
// not evidence of backend authorization, upstream identity binding, or real UV.
// Run after dx build; requires Node + Playwright and its Chromium runtime.
// Default: reduced motion. Pass --normal-motion to exercise animated native dialogs.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.ATTOS_TEST_PLAYWRIGHT_MODULE || 'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
// BUILD_DIR is the compiled public directory (containing index.html and assets/).
const publicDir=path.resolve(process.env.BUILD_DIR || path.join(root,'target/dx/attos-dashboard-ui/release/web/public'));
assert.ok(fs.existsSync(path.join(publicDir,'index.html')),'build the Dioxus release UI first');
const csp=fs.readFileSync(path.join(root,'src/dashboard/http/ui.rs'),'utf8').match(/const CSP: &str = "([^"]+)"/)[1];
const uid=n=>`00000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const user={id:uid(1),username:'synthetic-admin',is_admin:true,active:true,must_change_password:true};
const other={...user,id:uid(9),username:'synthetic-other',is_admin:false,active:false,must_change_password:false};
let runPlan=['queued','running','success'];
let verified=false,cookieSaved=false,statusMode=null,runProgress=[],passkeyDeletes=0,passkeys=[],credentialId,failLogout=false,allowLogin,allowSettings,expirePasswordSession=false,sessionExpired=false;
const createdUsers=[];
const secondAdmin={...user,id:uid(14),username:'synthetic-second-admin',must_change_password:false};
let failCreate=false,adminDeletes=0,loginFailureStatus=0,failPatch=false,passkeyHold=null,releasePasskeys=()=>{},adminPatchHold=null,releaseAdminPatch=()=>{};
const rewardPatches=[];
const avatarIcon='https://act-webstatic.hoyoverse.com/game_record/genshin/character_side_icon/synthetic.png';
const itemIcon='https://act-upload.hoyolab.com/game_record/hkrpg/item_icon/synthetic.png';
const awardIcon='https://fastcdn.hoyoverse.com/static-resource-v2/synthetic-award.png';
const validGenerated=value=>typeof value==='string'&&/^[A-Za-z0-9_-]{24}$/.test(value);
const loginGate=new Promise(resolve=>{allowLogin=resolve;});
const settingsGate=new Promise(resolve=>{allowSettings=resolve;});
const counts=new Map();const failures=[];
const accounts=[{id:uid(3),visible:false,detected:true,game_uid:'600000003'},{id:uid(4),visible:true,detected:false,game_uid:'600000004'},{id:uid(5),visible:true,detected:true,game_uid:'600000005'}].map(a=>({...a,hoyolab_account_id:uid(2),game_kind:'genshin',region:'os_usa',last_detected_at:new Date().toISOString()}));
accounts.push(...[{id:uid(6),game_kind:'genshin',game_uid:'600000006'},{id:uid(7),game_kind:'starrail',game_uid:'700000007'},{id:uid(8),game_kind:'starrail',game_uid:'700000008'}].map(a=>({...accounts[2],...a})));
const rewardSettings=['genshin','starrail'].map((game_kind,i)=>({id:uid(20+i),game_kind,enabled:false,local_time:'06:00',auth_status:'ready',next_run_at:null,last_run:null,is_sign:!!i,total_sign_day:12+i,today_award:i?null:{name:'Primogem',count:20,icon:awardIcon},last_checked_at:new Date().toISOString(),error_code:null,error_at:null}));
const hoursAgo=h=>new Date(Date.now()-h*3600e3).toISOString();
// Backend error codes (gateway::error_code) must never be rendered raw in settings.
let sessions=[{id:uid(50),created_at:hoursAgo(2),last_seen_at:hoursAgo(0.1),expires_at:hoursAgo(-100),user_agent:'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36',current:true},{id:uid(51),created_at:hoursAgo(30),last_seen_at:null,expires_at:hoursAgo(-70),user_agent:'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1',current:false},{id:uid(52),created_at:hoursAgo(50),last_seen_at:null,expires_at:hoursAgo(-50),user_agent:null,current:false}];
const rewardRuns=[{id:uid(40),setting_id:uid(20),game_kind:'genshin',trigger:'scheduled',state:'failed',created_at:hoursAgo(20),ended_at:hoursAgo(19.9),attempt_count:1,attempts:[{number:1,outcome:'failed',error_code:'http:503',started_at:hoursAgo(20),finished_at:hoursAgo(19.9)}]}];
const json=(res,data,status=200)=>{res.writeHead(status,{'Content-Type':'application/json','Cache-Control':'no-store'});res.end(JSON.stringify(data));};
let origin;
const server=http.createServer(async(req,res)=>{
 try{
  const url=new URL(req.url,origin);const p=url.pathname;
  res.setHeader('X-Content-Type-Options','nosniff');res.setHeader('Cache-Control','no-store');
  if(p.startsWith('/api/v1/')){
   counts.set(p,(counts.get(p)||0)+1);
   let raw='';for await(const chunk of req){raw+=chunk;if(raw.length>16384)throw new Error('fixture request body too large');}
   const body=raw?JSON.parse(raw):null;
   if(req.method!=='GET')assert.equal(req.headers.origin,origin,'browser emits the real same-origin Origin');
   const actor=req.headers.cookie?.includes('attos_session=synthetic-a')?user:req.headers.cookie?.includes('attos_session=synthetic-b')?other:null;
   if(p==='/api/v1/auth/login'){
    if(loginFailureStatus)return json(res,{error:loginFailureStatus===403?'forbidden':'unavailable'},loginFailureStatus);
    const loginUser=body.username===user.username?user:body.username===other.username?other:null;
    if(!loginUser||body.password!=='synthetic-password-123')return json(res,{error:'unauthorized'},401);
    await loginGate;sessionExpired=false;res.setHeader('Set-Cookie',`attos_session=synthetic-${loginUser===user?'a':'b'}; HttpOnly; SameSite=Strict; Path=/`);return json(res,loginUser);
   }
   if(p==='/api/v1/auth/logout'){if(failLogout)return json(res,{error:'unavailable'},503);res.setHeader('Set-Cookie','attos_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0');return json(res,{});}
   if(p==='/api/v1/auth/passkey/options'){assert.equal(body.username,user.username);assert.ok(credentialId);return json(res,{challenge_id:uid(13),options:{publicKey:{challenge:Buffer.from('synthetic-assertion-challenge-32-bytes').toString('base64url'),rpId:'localhost',allowCredentials:[{type:'public-key',id:credentialId,transports:['internal']}],userVerification:'required',timeout:60000}}});}
   if(p==='/api/v1/auth/passkey/verify'){
    assert.equal(body.response.type,'public-key');assert.equal(body.response.id,credentialId);assert.equal(body.response.rawId,credentialId);
    assert.ok(body.response.response.clientDataJSON);assert.ok(body.response.response.authenticatorData);assert.ok(body.response.response.signature);
    assert.ok(body.response.response.userHandle===null||typeof body.response.response.userHandle==='string');
    res.setHeader('Set-Cookie','attos_session=synthetic-a; HttpOnly; SameSite=Strict; Path=/');return json(res,user);
   }
   if(!actor||sessionExpired)return json(res,{error:'unauthorized'},401);
   if(p==='/api/v1/me'&&req.method==='DELETE'){assert.deepEqual(body,{current_password:'wrong-synthetic-password',confirmation:user.username});return json(res,{error:'unauthorized'},401);}
   if(p==='/api/v1/me')return json(res,{...actor,onboarding:{password_change_required:actor.must_change_password,cookie_setup_complete:verified,cookie_status:verified?'verified':cookieSaved?'pending_verification':'not_configured',verification_available:true},capabilities:{reward_posting_available:true}});
   if(p==='/api/v1/me/hoyolab'){await settingsGate;return json(res,{id:cookieSaved||verified?uid(2):null,hoyolab_uid:verified?'123':null,status:verified?'verified':cookieSaved?'pending_verification':'not_configured',setup_complete:verified,updated_at:cookieSaved||verified?hoursAgo(1):null,verification_available:true});}
   if(p==='/api/v1/me/password'){
    assert.equal(req.method,'PUT');
    if(expirePasswordSession){sessionExpired=true;return json(res,{error:'unauthorized'},401);}
    // Never use value-printing assertions for request secrets.
    assert.ok(body.new_password==='synthetic-permanent-password-123');
    if(actor.must_change_password)assert.ok(!Object.hasOwn(body,'current_password'),'mandatory change omits current password');
    else {
     if(body.current_password==='wrong-synthetic-password')return json(res,{error:'unauthorized'},401);
     assert.ok(body.current_password==='synthetic-password-123');
    }
    actor.must_change_password=false;res.writeHead(204);return res.end();
   }
   // Intentionally empty/malformed success bodies exercise client-only acknowledgement policy, not the backend JSON contract.
   if(p==='/api/v1/me/hoyolab/cookie'){assert.equal(actor.must_change_password,false);assert.ok(body.cookie.startsWith('ltuid_v2=123;'));cookieSaved=true;res.writeHead(202,{'Content-Type':'application/json'});return res.end('ignored acknowledgement');}
   if(p==='/api/v1/me/hoyolab/sync'){assert.equal(req.method,'POST');assert.deepEqual(body,{});assert.ok(verified);res.writeHead(204);return res.end();}
   if(p==='/api/v1/me/hoyolab/verify'){assert.equal(req.method,'POST');assert.deepEqual(body,{});verified=true;res.writeHead(204);return res.end();}
   if(p==='/api/v1/me/passkeys/options')return json(res,{challenge_id:uid(10),options:{publicKey:{challenge:Buffer.from('synthetic-browser-challenge-32-bytes').toString('base64url'),rp:{id:'localhost',name:'Synthetic UI check'},user:{id:Buffer.from('synthetic-user').toString('base64url'),name:user.username,displayName:user.username},pubKeyCredParams:[{type:'public-key',alg:-7}],timeout:60000,authenticatorSelection:{userVerification:'required',residentKey:'preferred'},attestation:'none'}}});
   if(p==='/api/v1/me/passkeys/verify'){
    assert.ok(body.response.response.clientDataJSON);assert.ok(body.response.response.attestationObject);assert.equal(body.response.type,'public-key');
    assert.equal(typeof body.response.id,'string');assert.match(body.response.id,/^[A-Za-z0-9_-]+$/);
    assert.equal(body.response.id,body.response.rawId,'native credential id matches base64url rawId');
    assert.equal(Buffer.from(body.response.rawId,'base64url').toString('base64url'),body.response.rawId);
    credentialId=body.response.rawId;passkeys=[{id:uid(11),label:'Synthetic browser key',created_at:new Date().toISOString(),last_used_at:null}];return json(res,{});
   }
   if(p===`/api/v1/me/passkeys/${uid(11)}`&&req.method==='PATCH'){assert.deepEqual(Object.keys(body),['label']);passkeys[0].label=body.label;res.writeHead(204);return res.end();}
   if(p===`/api/v1/me/passkeys/${uid(11)}`){assert.equal(req.method,'DELETE');passkeyDeletes++;passkeys=[];res.writeHead(204);return res.end();}
   if(p==='/api/v1/me/sessions'){assert.equal(req.method,'GET');return json(res,sessions);}
   if(p==='/api/v1/me/notifications'&&req.method==='GET')return json(res,{webhook_configured:false,webhook_hint:null,webhook_format:'discord',language:'ja',events:{},last_delivery_at:null,last_delivery_ok:null,resin_warn_hours:2,daily_reminder_time:'21:00',weekly_reminder_time:'15:00',push_public_key:null,push_subscriptions:0});
   if(p==='/api/v1/me/notifications'&&req.method==='PUT'){assert.ok(body.webhook_format);res.writeHead(204);return res.end();}
   if(p==='/api/v1/admin/notifications'&&req.method==='GET')return json(res,{webhook_configured:false,webhook_hint:null,webhook_format:'discord',language:'ja',events:{},last_delivery_at:null,last_delivery_ok:null});
   if(p==='/api/v1/admin/notifications'&&req.method==='PUT'){res.writeHead(204);return res.end();}
      if(p===`/api/v1/me/sessions/${uid(51)}`){assert.equal(req.method,'DELETE');sessions=sessions.filter(row=>row.id!==uid(51));res.writeHead(204);return res.end();}
   if(p==='/api/v1/me/sessions/revoke-others'){assert.equal(req.method,'POST');assert.deepEqual(body,{});const revoked=sessions.filter(row=>!row.current).length;sessions=sessions.filter(row=>row.current);return json(res,{revoked});}
   if(p==='/api/v1/me/passkeys'){if(passkeyHold)await passkeyHold;return json(res,passkeys);}
   if(p==='/api/v1/server-settings'){await settingsGate;return json(res,{timezone_offset:'+09:00',session_ttl_days:7});}
   if(p==='/api/v1/admin/users'){
    if(req.method==='POST'){assert.ok(actor===user);assert.ok(body.username==='synthetic-created');assert.ok(validGenerated(body.temporary_password),'24-character WebCrypto candidate');if(failCreate)return json(res,{error:'unavailable'},503);const created={...other,id:uid(12),username:body.username,must_change_password:true};createdUsers.push(created);return json(res,created,201);}
    return json(res,[user,other,secondAdmin,...createdUsers].map(row=>({...row,created_at:hoursAgo(100),link_status:row===other?'verified':'not_configured',automatic_claims:row===other?2:0,last_login_at:row===user?hoursAgo(1):null})));
   }
   if(p==='/api/v1/admin/audit'){assert.equal(req.method,'GET');return json(res,[{id:uid(60),action:'user_created',created_at:hoursAgo(1),actor:user.username,target:other.username},{id:uid(61),action:'signed_in',created_at:hoursAgo(2),actor:null,target:null}]);}
   if(p===`/api/v1/admin/users/${uid(12)}/temporary-password`){assert.equal(req.method,'PUT');assert.ok(validGenerated(body.temporary_password));res.writeHead(204);return res.end();}
   if(p===`/api/v1/admin/users/${other.id}`){
    assert.ok(actor===user);
    if(req.method==='PATCH'){if(adminPatchHold)await adminPatchHold;assert.deepEqual(body,{active:true});other.active=true;res.writeHead(204);return res.end();}
    assert.equal(req.method,'DELETE');assert.ok(body.confirmation===other.username);assert.ok(body.current_password==='synthetic-password-123');adminDeletes++;res.writeHead(202);return res.end();
   }
   if(p==='/api/v1/game-accounts')return json(res,accounts);
   const visibility=accounts.find(a=>p===`/api/v1/game-accounts/${a.id}`);
   if(visibility){assert.equal(req.method,'PATCH');assert.deepEqual(Object.keys(body),['visible']);if(failPatch)return json(res,{error:'unavailable'},503);visibility.visible=body.visible;res.writeHead(204);return res.end();}
   const account=accounts.find(a=>p===`/api/v1/game-accounts/${a.id}/status`);
   if(account&&statusMode==='failed')return json(res,{...account,data:null,fetched_at:null,error_code:'network',error_at:new Date().toISOString(),stale:true});
   // 7h 12m minus a few seconds: the countdown rounds up to the displayed minute.
   if(account&&statusMode==='recovering')return json(res,{...account,data:{current_stamina:150,max_stamina:300,stamina_full_at:new Date(Date.now()+(7*3600+12*60-5)*1000).toISOString()},fetched_at:new Date().toISOString(),error_code:null,error_at:null,stale:false});
   if(account&&statusMode==='stale')return json(res,{...account,data:{current_stamina:5,max_stamina:300},fetched_at:new Date(Date.now()-6*3600e3-60e3).toISOString(),error_code:'http:503',error_at:new Date().toISOString(),stale:true});
   if(account){assert.ok(account.visible&&account.detected);return json(res,{...account,data:account.game_kind==='genshin'?{current_resin:actor===user?(account.id===uid(5)?0:31):77,max_resin:200,finished_task_num:0,total_task_num:4,resin_full_at:account.id===uid(5)?'2026-10-17T12:38:00Z':'invalid-date',current_home_coin:510,max_home_coin:2400,home_coin_full_at:'2026-10-19T08:38:40Z',remain_resin_discount_num:2,resin_discount_num_limit:3,max_expedition_num:5,transformer_obtained:true,transformer_ready:true,expeditions:[{status:'Ongoing',remained_time:'30354',avatar_side_icon:avatarIcon},{status:'Finished',remained_time:'0',avatar_side_icon:avatarIcon}]}:{current_stamina:account.id===uid(7)?12:24,max_stamina:300,weekly_cocoon_cnt:1,weekly_cocoon_limit:3,current_rogue_score:2000,max_rogue_score:14000,stamina_full_at:account.id===uid(7)?'2026-10-17T16:52:31Z':null,expeditions:account.id===uid(7)?[{name:'Synthetic assignment',status:'Ongoing',remaining_time:1200,item_url:itemIcon}]:[]},fetched_at:new Date().toISOString(),error_code:null,error_at:null,stale:false});}
   if(p==='/api/v1/reward-settings'){
    assert.equal(req.method,'GET');
    // Each read after an accepted manual run advances it one state, like the worker would.
    const response=JSON.parse(JSON.stringify(rewardSettings));
    if(runProgress.length>1){runProgress.shift();rewardSettings[0].last_run={id:uid(30),state:runProgress[0],ended_at:runProgress[0]==='success'?new Date().toISOString():null};}
    return json(res,response);
   }
   const setting=rewardSettings.find(s=>p===`/api/v1/reward-settings/${s.id}`);
   if(setting){assert.equal(req.method,'PATCH');assert.deepEqual(Object.keys(body).sort(),['enabled','local_time']);if(failPatch)return json(res,{error:'unavailable'},503);rewardPatches.push({id:setting.id,...body});Object.assign(setting,body,{next_run_at:new Date(Date.now()+86400000).toISOString()});res.writeHead(204);return res.end();}
   if(p===`/api/v1/reward-settings/${uid(20)}/run`){assert.equal(req.method,'POST');assert.deepEqual(body,{});rewardSettings[0].last_run={id:uid(30),state:'queued',ended_at:null};runProgress=[...runPlan];res.writeHead(202);return res.end();}
   if(p==='/api/v1/reward-runs')return json(res,rewardRuns);
   throw new Error('Unexpected implemented-API request: '+req.method+' '+p);
  }
  if(p==='/openapi.json'){assert.equal(req.method,'GET');counts.set(p,(counts.get(p)||0)+1);return json(res,JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')));}
  let file;
  if(p==='/settings'){res.writeHead(308,{'Location':'/settings/games'});return res.end();}
  if(['/','/login','/dashboard','/settings/games','/settings/bonus','/settings/link','/settings/security','/settings/notifications','/settings/delete','/admin','/docs'].includes(p)){file=path.join(publicDir,'index.html');res.setHeader('Content-Type','text/html');res.setHeader('Content-Security-Policy',csp);}
  else if(p.startsWith('/assets/')){file=path.resolve(publicDir,'.'+p);assert.ok(file.startsWith(path.join(publicDir,'assets')+path.sep));res.setHeader('Content-Type',({'.wasm':'application/wasm','.js':'text/javascript','.css':'text/css','.png':'image/png'})[path.extname(file)]||'application/octet-stream');}
  else {res.writeHead(404);return res.end();}
  res.end(fs.readFileSync(file));
 }catch{failures.push(`${req.method} ${new URL(req.url,origin).pathname}: fixture assertion failed`);json(res,{error:'fixture_failure'},500);}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));origin=`http://localhost:${server.address().port}`;
let browser,page,stage='login';const errors=[];
try{
 browser=await chromium.launch({headless:true,args:['--no-sandbox']});
 const context=await browser.newContext({viewport:{width:1280,height:900},timezoneId:'Asia/Tokyo',colorScheme:'dark',reducedMotion:process.argv.includes('--normal-motion')?'no-preference':'reduce'});
 context.setDefaultTimeout(10000);
 await context.addInitScript(()=>{if(!localStorage.getItem('ahd-language'))localStorage.setItem('ahd-language','en');});
 await context.route('**/*',route=>{
  const url=route.request().url();
  if(url===avatarIcon||url===itemIcon||url===awardIcon)return route.fulfill({status:200,contentType:'image/png',body:fs.readFileSync(path.join(root,'dashboard-ui/assets/favicon.png'))});
  return new URL(url).origin===origin?route.continue():route.abort();
 });
 page=await context.newPage();
 page.on('pageerror',()=>errors.push('browser page error'));
 page.on('console',message=>{if(message.type()==='error'&&/Content Security Policy|unsafe-eval|inline style/i.test(message.text()))errors.push('CSP violation');});
 const button=name=>page.getByRole('button',{name,exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('fieldset.app-shell')?.disabled===false);
 const dashboard=async()=>{await page.getByRole('heading',{name:'Dashboard',exact:true}).waitFor();await idle();await page.waitForURL(origin+'/dashboard');};
 const openMenu=async()=>{await page.locator('#ahd-menu-trigger').click();await page.locator('#ahd-menu[open]').waitFor();await page.waitForFunction(()=>Math.abs(document.getElementById('ahd-menu').getBoundingClientRect().right-innerWidth)<1);await page.locator('#ahd-menu').evaluate(async element=>{await Promise.all(element.getAnimations().map(animation=>animation.finished.catch(()=>{})));});};
 const menu=async name=>{await openMenu();const dialog=await page.locator('#ahd-menu').elementHandle();await page.locator('#ahd-menu').getByRole('button',{name,exact:true}).click();await page.locator('#ahd-menu[open]').waitFor({state:'hidden'});await dialog.evaluate(async element=>{await Promise.all(element.getAnimations().map(animation=>animation.finished.catch(()=>{})));});await dialog.dispose();await idle();};
 const settings=async key=>{await menu('Settings');await page.locator('#settings-tab-'+key).click();await idle();};
 const signIn=async actor=>{await page.locator('#login-username').fill(actor.username);await page.locator('#login-password').fill('synthetic-password-123');await button('Sign in').click();};
 const change=async(current)=>{if(current!==undefined)await page.locator('#current-password').fill(current);for(const id of ['new-password','confirm-password'])await page.locator('#'+id).fill('synthetic-permanent-password-123');await button('Change password').click();await idle();};
 // Test every owning PasswordInput, without reading a secret into assertion diagnostics.
 const eyes=async ids=>{
  for(const id of ids){
   const input=page.locator('#'+id),eye=page.locator('button[aria-controls="'+id+'"]');
   await input.fill('synthetic-eye-password');
   assert.equal(await input.getAttribute('type'),'password');
   assert.equal(await eye.getAttribute('aria-label'),'Show password');
   await eye.click();assert.equal(await input.getAttribute('type'),'text');
   assert.equal(await eye.getAttribute('aria-pressed'),'true');
   await input.fill('synthetic-eye-password-two');
   assert.equal(await input.getAttribute('type'),'text','nonempty typing preserves explicit reveal');
   await eye.click();await page.waitForFunction(id=>document.getElementById(id)?.type==='password',id);
   await eye.click();await page.waitForFunction(id=>document.getElementById(id)?.type==='text',id);
   await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
   await page.waitForFunction(id=>document.getElementById(id)?.type==='password',id);
   await eye.click();
   await page.evaluate(()=>{Object.defineProperty(document,'hidden',{configurable:true,value:true});document.dispatchEvent(new Event('visibilitychange'));delete document.hidden;});
   await page.waitForFunction(id=>document.getElementById(id)?.type==='password',id);
   await eye.click();await page.keyboard.press('Tab');
   await page.waitForFunction(id=>document.getElementById(id)?.type==='password',id);
   await input.fill('');
  }
 };
 await page.goto(origin);
 await page.locator('#login-username').waitFor();
 await page.waitForURL(origin+'/login');
 assert.equal(new URL(page.url()).hash,'','auth routes do not use hashes');
 await page.goto(origin+'/settings/security');
 await page.locator('#login-username').waitFor();await page.waitForURL(origin+'/login');
 assert.equal(await page.locator('#current-password').count(),0,'direct private URL cannot bypass login');
 await page.locator('.wordmark').getByText('AHD',{exact:true}).waitFor();
 // The supplied central motif is a real transparent asset, not a CSS background/mask.
 const brandIcon=page.locator('.wordmark img');
 assert.equal(await brandIcon.getAttribute('alt'),'','adjacent AHD text names the brand once');
 assert.equal(await page.locator('head link[rel="icon"]').getAttribute('type'),'image/png');
 assert.equal(await page.locator('head link[rel="icon"]').getAttribute('sizes'),'32x32');
 const iconPixels=await page.evaluate(async()=>{
  const sources=[document.querySelector('.wordmark img').src,document.querySelector('head link[rel="icon"]').href];
  return Promise.all(sources.map(async src=>{
   const image=new Image();image.src=src;await image.decode();
   const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;
   const context=canvas.getContext('2d');context.drawImage(image,0,0);
   const pixels=context.getImageData(0,0,image.width,image.height).data;
   const alpha=(x,y)=>pixels[(y*image.width+x)*4+3];
   return {size:[image.width,image.height],corners:[alpha(0,0),alpha(image.width-1,0),alpha(0,image.height-1),alpha(image.width-1,image.height-1)],center:alpha(image.width/2,image.height/2),softEdge:pixels.some((value,index)=>index%4===3&&value>0&&value<255)};
  }));
 });
 for(const [i,size] of [256,32].entries()){
  assert.deepEqual(iconPixels[i].size,[size,size]);assert.deepEqual(iconPixels[i].corners,[0,0,0,0]);
  assert.equal(iconPixels[i].center,255);assert.ok(iconPixels[i].softEdge,'antialiased transparency survives production bundling');
 }
 const brandBox=await brandIcon.boundingBox(),brandTextBox=await page.locator('.wordmark span').boundingBox();
 assert.equal(brandBox.width,36);assert.equal(brandBox.height,36);
 assert.ok(brandBox.x+brandBox.width<brandTextBox.x&&Math.abs(brandBox.y+brandBox.height/2-brandTextBox.y-brandTextBox.height/2)<1,'icon precedes and centers on AHD');
 await page.waitForFunction(()=>document.fonts.size>=2);
 assert.ok(await page.evaluate(async()=>{await document.fonts.ready;return [...document.fonts].filter(f=>f.status==='loaded').length>=2;}),'self-hosted UI and wordmark fonts load');
 assert.equal(await page.getByText('Please sign in.',{exact:true}).count(),0,'no redundant login instructions');
 const usernameBox=await page.locator('#login-username').boundingBox(),passwordBox=await page.locator('#login-password').boundingBox(),submitBox=await button('Sign in').boundingBox();
 assert.ok(usernameBox.height>=48&&passwordBox.height>=48,'default text and password controls share the AHD size');
 assert.ok(Math.abs(submitBox.width-passwordBox.width)<1&&submitBox.y-passwordBox.y-passwordBox.height>=24,'full-width submit and password gap');
 assert.ok(await button('Sign in').evaluate(e=>getComputedStyle(e).backgroundImage.includes('linear-gradient')),'stable data attributes style the CSS-module button');
 assert.equal(await page.locator('#login-username').count(),1,'both methods share one username');
 await eyes(['login-password']);
 await page.locator('#login-password').fill('synthetic-method-clear');
 await page.locator('#login-method-passkey').check();
 assert.equal(await page.locator('#login-password').count(),0);
 await page.locator('#login-method-password').check();
 assert.ok(await page.locator('#login-password').evaluate(e=>e.value===''));
 // Unknown usernames and wrong passwords share one non-enumerating error.
 for(const language of ['en','ja']){
  if(language==='ja'){
   await page.evaluate(()=>localStorage.setItem('ahd-language','ja'));await page.reload();
   await page.locator('#login-username').waitFor();await idle();
  }
  for(const [username,password] of [['synthetic-missing-user','synthetic-password-123'],[user.username,'synthetic-wrong-password']]){
   await page.locator('#login-username').fill(username);await page.locator('#login-password').fill(password);
   await button(language==='ja'?'ログイン':'Sign in').click();
   await page.locator('.status.error').getByText(language==='ja'?'ユーザー名かパスワードが違います。':'The username or password is incorrect.',{exact:true}).waitFor();
   await idle();assert.equal(await page.locator('#login-username').count(),1);
   assert.equal(await page.locator('#login-username').inputValue(),username,'a failed sign-in keeps the username');
   assert.ok(await page.locator('#login-password').evaluate(e=>e.value===''),'a failed sign-in clears the password');
  }
 }
 await page.evaluate(()=>localStorage.setItem('ahd-language','en'));await page.reload();
 await page.locator('#login-username').waitFor();await idle();
 for(const [status,message] of [[403,'You do not have access to this action.'],[503,'This feature is not available right now.']]){
  loginFailureStatus=status;await signIn(user);await page.getByText(message,{exact:true}).waitFor();await idle();
  assert.equal(await page.getByText('The username or password is incorrect.',{exact:true}).count(),0);
 }
 loginFailureStatus=0;
 await page.route(origin+'/api/v1/auth/login',route=>route.abort(),{times:1});
 await signIn(user);await page.getByText('The server could not be reached.',{exact:true}).waitFor();await idle();
 const loginAttemptsBefore=counts.get('/api/v1/auth/login'),sessionReadsBefore=counts.get('/api/v1/me');
 await page.locator('#login-username').fill(user.username);
 await page.locator('#login-password').fill('synthetic-password-123');
 const loginRequest=page.waitForRequest(origin+'/api/v1/auth/login');
 await button('Sign in').click();await loginRequest;
 assert.ok(await button('Sign in').isDisabled());
 assert.ok(await page.locator('#login-method-passkey').isDisabled());
 // Native disabled controls plus the synchronous submission reservation.
 await page.locator('form').dispatchEvent('submit');
 const metadataRequest=page.waitForRequest(origin+'/api/v1/me/hoyolab');allowLogin();await metadataRequest;
 assert.ok(await button('Refresh setup status').isDisabled(),'bootstrap stays busy');
 await button('Refresh setup status').dispatchEvent('click');
 allowSettings();await idle();
 assert.equal(counts.get('/api/v1/auth/login'),loginAttemptsBefore+1);
 assert.equal(counts.get('/api/v1/auth/passkey/options')||0,0);
 assert.equal(counts.get('/api/v1/me'),sessionReadsBefore+1);
 assert.equal(await page.locator('#ahd-menu-trigger').count(),0,'mandatory setup has no ordinary navigation');
 // Setup progress is a read-only list, not tabs: no tab roles, buttons or focus stops.
 const steps=page.locator('ol.setup-steps');
 assert.equal(await steps.getAttribute('aria-label'),'Setup progress');
 assert.equal(await page.getByRole('tablist').count(),0,'setup has no tablist');
 assert.equal(await steps.locator('[role=tab],button,a,[tabindex]').count(),0,'steps are not interactive');
 assert.deepEqual(await steps.locator('li').evaluateAll(items=>items.map(item=>[item.className,item.getAttribute('aria-current'),item.tabIndex])),[['setup-step current','step',-1],['setup-step upcoming',null,-1]]);
 assert.equal(await steps.locator('li[aria-current=step] .step-label').innerText(),'Change password');
 assert.equal(await steps.locator('.visually-hidden').count(),0,'nothing is done yet');
 assert.equal(await steps.locator('li').first().evaluate(e=>getComputedStyle(e).cursor),'default');
 assert.equal(await steps.locator('li').first().evaluate(e=>getComputedStyle(e).borderBottomStyle),'none','no tab-like underline');
 assert.equal(await button('Sign out').count(),1,'setup permits logout');
 assert.equal(await page.locator('#current-password').count(),0,'mandatory setup never requests a login secret');
 assert.equal(await page.locator('#hoyolab-cookie').count(),0,'password gate hides Cookie actions');
 await page.locator('a.wordmark').click();await idle();
 assert.equal(await page.locator('#new-password').count(),1,'brand navigation cannot bypass onboarding');
 assert.equal(await page.getByRole('heading',{name:'Dashboard',exact:true}).count(),0);
 assert.equal(counts.get('/api/v1/game-accounts')||0,0);
 await eyes(['new-password','confirm-password']);
 // Requirements are visible before typing and linked to the field.
 assert.match(await page.locator('#new-password-rules').innerText(),/12–256 bytes/);
 assert.equal(await page.locator('#new-password').getAttribute('aria-describedby'),'new-password-rules');
 assert.equal(await page.locator('#new-password-error').count(),0,'no error before input');
 await page.locator('#new-password').fill('short');await page.locator('#new-password').press('Tab');
 assert.equal(await page.locator('#new-password-error').innerText(),'Currently 5 bytes; use 12–256 bytes.');
 assert.equal(await page.locator('#new-password').getAttribute('aria-invalid'),'true');
 assert.equal(await page.locator('#new-password').getAttribute('aria-describedby'),'new-password-rules new-password-error');
 // Confirm mismatch is reported beside the field and makes no request.
 await page.locator('#new-password').fill('synthetic-permanent-password-123');
 await page.locator('#confirm-password').fill('synthetic-mismatch');
 await button('Change password').click();await idle();
 assert.equal(counts.get('/api/v1/me/password')||0,0);
 assert.equal(await page.locator('#new-password-error').count(),0,'a valid length clears its error');
 assert.equal(await page.locator('#confirm-password-error').innerText(),'The confirmation does not match the new password.');
 assert.equal(await page.locator('#confirm-password').getAttribute('aria-invalid'),'true');
 assert.equal(await page.locator('#confirm-password').getAttribute('aria-describedby'),'confirm-password-error');
 assert.equal(await page.locator('#new-password').inputValue(),'synthetic-permanent-password-123','invalid submissions keep the typed values');
 assert.ok(await page.locator('#confirm-password').evaluate(e=>e===document.activeElement),'focus moves to the problem field');
 await change();
 await page.locator('#hoyolab-cookie').waitFor();
 assert.deepEqual(await steps.locator('li').evaluateAll(items=>items.map(item=>[item.className,item.getAttribute('aria-current')])),[['setup-step done',null],['setup-step current','step']]);
 assert.equal(await steps.locator('li.done').evaluate(e=>e.textContent),'Change password (done)','done state has accessible text');
 assert.equal(await steps.locator('li.done .visually-hidden').evaluate(e=>e.getBoundingClientRect().width<=1),true);
 assert.equal(await page.locator('#new-password').count(),0);
 assert.equal(counts.get('/api/v1/me/password'),1);
 assert.equal(counts.get('/api/v1/game-accounts')||0,0,'incomplete real linkage still gates dashboard');
 assert.equal(counts.get('/api/v1/reward-settings')||0,0);
 assert.ok(await button('Sync UIDs').isDisabled());
 const cookieHelp=await page.locator('#hoyolab-cookie-help').innerText();
 for(const key of ['ltoken_v2','ltuid_v2','ltmid_v2','hoyolab.com','Cookie header'])assert.ok(cookieHelp.includes(key),'cookie help names '+key);
 assert.equal(await page.locator('#hoyolab-cookie').getAttribute('aria-describedby'),'hoyolab-cookie-help');
 assert.equal(await page.locator('#hoyolab-step').innerText(),'Paste your HoYoLAB cookie and save it.');
 assert.equal(await button('Save').getAttribute('data-style'),'primary');
 assert.equal(await button('Verify').getAttribute('data-style'),'secondary');
 assert.ok(await button('Verify').isDisabled(),'nothing to verify before saving');
 assert.equal(await button('Sync UIDs').getAttribute('aria-describedby'),'hoyolab-sync-hint');
 assert.equal(await page.locator('#hoyolab-sync-hint').innerText(),'Sync UIDs becomes available after verification.');
 assert.equal(await page.getByText('Available',{exact:true}).count(),0,'no unexplained availability metric');
 // A Cookie missing a required name is caught before it is sent.
 await page.locator('#hoyolab-cookie').fill('ltuid_v2=123; ltoken_v2=synthetic-browser-token');
 await button('Save').click();
 assert.equal(await page.locator('#hoyolab-cookie-error').innerText(),'These Cookies are missing: ltmid_v2');
 assert.equal(await page.locator('#hoyolab-cookie').getAttribute('aria-invalid'),'true');
 assert.equal(await page.locator('#hoyolab-cookie').getAttribute('aria-describedby'),'hoyolab-cookie-help hoyolab-cookie-error');
 assert.equal(counts.get('/api/v1/me/hoyolab/cookie')||0,0,'an incomplete Cookie is not sent');
 await page.locator('#hoyolab-cookie').fill('ltuid_v2=123; ltoken_v2=synthetic-browser-token; ltmid_v2=synthetic-mid');
 assert.equal(await page.locator('#hoyolab-cookie-error').count(),0,'a complete Cookie clears the error');
 const cookieField=await page.locator('#hoyolab-cookie').elementHandle();
 const meBeforeSave=counts.get('/api/v1/me');
 await button('Save').click();await idle();
 assert.ok(await page.locator('#hoyolab-cookie').evaluate(e=>e.value===''));
 await page.locator('.status.success').getByText('Cookie save accepted. Run Verify next.',{exact:true}).waitFor();
 assert.equal(await page.locator('#hoyolab-step').innerText(),'Cookie saved but not verified yet. Run Verify next.');
 assert.equal(await button('Verify').getAttribute('data-style'),'primary','a saved, unverified cookie makes Verify the primary action');
 assert.equal(await button('Save').getAttribute('data-style'),'secondary');
 assert.equal(await button('Verify').getAttribute('aria-describedby'),'hoyolab-step');
 assert.equal(counts.get('/api/v1/me'),meBeforeSave+1,'the setup gate is re-read after saving');
 assert.ok(await cookieField.evaluate(e=>e.isConnected),'refreshing the gate keeps the setup form mounted (no Loading screen)');
 await cookieField.dispose();
 await button('Verify').click();await dashboard();
 await page.locator('.status.success').getByText('Verified; game accounts are synced.',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/me/hoyolab/verify'),1);
 stage='dashboard';
 // Dark desktop renders both real game cards, not a screenshot fixture.
 const genshin=page.locator('#game-panel-genshin'),starrail=page.locator('#game-panel-starrail');
 await genshin.locator('.resource strong').getByText('0',{exact:true}).waitFor();
 await starrail.locator('.resource strong').getByText('12',{exact:true}).waitFor();
 assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 assert.equal(await genshin.getAttribute('role'),'region');
 assert.equal(await genshin.locator('time.recovery-at').innerText(),'10/17 21:38 JST');
 assert.equal(await starrail.locator('time.recovery-at').innerText(),'10/18 01:52 JST','local date rollover uses 24-hour time');
 assert.match(await genshin.locator('time.recovery-at').evaluate(e=>getComputedStyle(e).fontFamily),/Orbitron/);
 assert.equal(await genshin.locator('.recovery-in').first().innerText(),'Fully recovered','a passed estimate is not a negative countdown');
 assert.doesNotMatch(await genshin.locator('.recovery-in').first().evaluate(e=>getComputedStyle(e).fontFamily),/Orbitron/);
 assert.equal(await genshin.locator('.recovery-in').nth(1).innerText(),'At cap','a passed currency estimate states the cap, not recovery');
 // Freshness is always visible beside Refresh; weekly allowances and the transformer show beside dailies.
 assert.equal(await genshin.locator('.freshness').innerText(),'Updated just now');
 assert.equal(await genshin.locator('.metric').filter({hasText:'Weekly boss discounts'}).locator('dd').innerText(),'2/3 left');
 assert.equal(await genshin.locator('.metric').filter({hasText:'Parametric Transformer'}).locator('dd').innerText(),'Ready');
 assert.equal(await starrail.locator('.metric').filter({hasText:'Echo of War'}).locator('dd').innerText(),'1/3 left');
 assert.equal(await starrail.locator('.metric').filter({hasText:'Simulated Universe'}).locator('dd').innerText(),'2000/14000');
 assert.equal(await genshin.locator('.expeditions-heading .muted').innerText(),'1/5 done');
 // Card order: UID, primary resource with full recovery, other stats, then the login bonus.
 for(const panel of [genshin,starrail]){
  const order=await panel.evaluate(card=>['.uid-select','.resource','.recovery-at','.metrics','.bonus-section'].map(selector=>card.querySelector(selector)).map((node,i,nodes)=>i===0||Boolean(nodes[i-1].compareDocumentPosition(node)&Node.DOCUMENT_POSITION_FOLLOWING)));
  assert.deepEqual(order,[true,true,true,true,true],'dashboard card order');
  assert.ok(await panel.locator('.bonus-section #automation-'+(panel===genshin?'genshin':'starrail')).count(),'bonus toggle lives in the last section');
 }
 const expeditionRows=genshin.locator('.expedition');
 assert.equal(await expeditionRows.count(),2);
 assert.equal((await expeditionRows.nth(0).innerText()).replace(/\s+/g,' ').trim(),'Ongoing Remaining: 8h 26m');
 assert.equal((await expeditionRows.nth(1).innerText()).trim(),'Finished','a finished expedition shows no zero remaining time');
 assert.doesNotMatch(await genshin.locator('time.estimate-at').evaluate(e=>getComputedStyle(e).fontFamily),/Orbitron/);
 assert.ok(!(await genshin.locator('time.estimate-at').innerText()).includes('T08:38:40'),'currency estimate is not raw ISO');
 assert.equal(await genshin.locator('.expedition-icon').first().getAttribute('src'),avatarIcon);
 assert.equal(await genshin.locator('.expedition-icon').first().getAttribute('alt'),'Expedition 1');
 assert.equal(await genshin.locator('.expedition-icon').nth(1).getAttribute('alt'),'Expedition 2');
 assert.equal(await genshin.locator('.expedition-icon').first().getAttribute('referrerpolicy'),'no-referrer');
 assert.equal(await starrail.locator('.expeditions').count(),0,'Star Rail hides expeditions even when its payload contains assignments');
 await page.waitForFunction(()=>[...document.querySelectorAll('.expedition img')].every(image=>image.complete&&image.naturalWidth>0),'official-host icons load under production CSP');
 assert.equal(await starrail.getByRole('heading',{name:'Expeditions',exact:true}).count(),0);
 await page.screenshot({path:path.join(root,'target/ui-dashboard-desktop.png'),fullPage:true});
 await genshin.locator('img.expedition-icon').first().dispatchEvent('error');
 await genshin.locator('span.expedition-icon[role=img]').waitFor();
 assert.equal(await genshin.locator('span.expedition-icon').getAttribute('aria-label'),'Expedition 1','failed icon keeps its accessible label');
 const gbox=await genshin.boundingBox(),sbox=await starrail.boundingBox();
 assert.ok(Math.abs(gbox.y-sbox.y)<2&&sbox.x>gbox.x+gbox.width,'desktop dual columns');
 assert.equal(await page.locator('#selected-genshin option').count(),2);
 assert.equal(await page.locator('#selected-starrail option').count(),2);
 assert.ok(await genshin.locator('.status-metadata').isHidden(),'technical data is collapsed by default');
 await genshin.getByText('Fetch details',{exact:true}).click();
 assert.ok(await genshin.locator('.status-metadata').isVisible(),'all stored metadata stays available');
 await genshin.getByText('Fetch details',{exact:true}).click();
 for(const n of [3,4])assert.equal(counts.get('/api/v1/game-accounts/'+uid(n)+'/status')||0,0);
 await page.locator('#selected-genshin').selectOption(uid(6));
 await genshin.locator('.resource strong').getByText('31',{exact:true}).waitFor();
 assert.equal(await genshin.locator('time.recovery-at').count(),0,'invalid estimate is not rendered as a date');
 assert.equal(await page.locator('#selected-starrail').inputValue(),uid(7));
 await page.locator('#selected-starrail').selectOption(uid(8));
 await starrail.locator('.resource strong').getByText('24',{exact:true}).waitFor();
 assert.equal(await starrail.locator('time.recovery-at').count(),0,'missing estimate is not invented');
 assert.equal(await starrail.locator('.expeditions').count(),0,'Star Rail also has no empty expedition section');
 assert.equal(await page.locator('#selected-genshin').inputValue(),uid(6));
 // A failed fetch without data reports only the failure; stale data states its age.
 statusMode='failed';await starrail.getByRole('button',{name:'Refresh',exact:true}).click();
 await starrail.getByText('Could not fetch the status (Could not reach HoYoLAB)',{exact:true}).waitFor();
 for(const contradiction of ['not the latest','may be out of date','No daily summary','network'])assert.ok(!(await starrail.innerText()).includes(contradiction),'failed fetch without data: '+contradiction);
 statusMode='stale';await starrail.getByRole('button',{name:'Refresh',exact:true}).click();
 await starrail.getByText('Data from 6 h ago (HoYoLAB is temporarily unavailable)',{exact:true}).waitFor();
 assert.ok(!(await starrail.innerText()).includes('http:503'),'raw codes stay out of the visible card');
 statusMode='recovering';await starrail.getByRole('button',{name:'Refresh',exact:true}).click();
 await starrail.locator('.resource strong').getByText('150',{exact:true}).waitFor();
 assert.equal(await starrail.locator('.recovery-in').innerText(),'in 7h 12m','full recovery also states the time left');
 assert.match(await starrail.locator('time.recovery-at').innerText(),/^\d\d\/\d\d \d\d:\d\d JST$/);
 statusMode=null;await starrail.getByRole('button',{name:'Refresh',exact:true}).click();
 await starrail.locator('.resource strong').getByText('24',{exact:true}).waitFor();
 assert.equal(await starrail.locator('.warning').count(),0);
 await genshin.getByRole('switch').check();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(20),enabled:true,local_time:'06:00'});
 assert.equal(rewardSettings[1].enabled,false,'automation PATCH cannot change other game');
 // A rejected PATCH leaves no re-render, so the DOM must be reset to the stored value.
 failPatch=true;
 const starrailSwitch=starrail.getByRole('switch');
 await starrailSwitch.click();await idle();
 await page.getByText('This feature is not available right now.',{exact:true}).waitFor();
 assert.equal(await starrailSwitch.isChecked(),false,'failed automation PATCH resets the switch');
 failPatch=false;
 // Drawer is native modal, right aligned, no content shift, keyboard/focus safe.
 await page.evaluate(()=>scrollTo(0,0));
 const before=await page.locator('main.container').boundingBox();
 await openMenu();
 const drawer=page.locator('#ahd-menu'),box=await drawer.boundingBox();
 const hoyolabLink=drawer.getByRole('link',{name:'HoYoLAB',exact:true});
 assert.equal(await hoyolabLink.getAttribute('href'),'https://www.hoyolab.com/home');
 assert.equal(await hoyolabLink.getAttribute('target'),'_blank');
 assert.equal(await drawer.getByRole('button',{name:'HoYoLAB link',exact:true}).count(),0);
 const logoutButton=drawer.getByRole('button',{name:'Sign out',exact:true});
 const logoutBefore=await logoutButton.boundingBox();
 await page.keyboard.press('Tab');await logoutButton.focus();
 const logoutAfter=await logoutButton.boundingBox();
 assert.deepEqual(logoutAfter,logoutBefore,'logout keyboard focus does not move its box');
 const logoutGroup=await drawer.locator('.menu-logout-group').boundingBox();
 assert.ok(Math.abs((logoutAfter.y-logoutGroup.y)-(logoutGroup.y+logoutGroup.height-logoutAfter.y-logoutAfter.height))<1,'logout has equal space between its dividers');
 await page.screenshot({path:path.join(root,'target/ui-menu-desktop.png'),fullPage:true});
 assert.equal(await logoutButton.evaluate(e=>getComputedStyle(e).outlineOffset),'-3px','focus ring stays inside drawer');
 await page.waitForFunction(()=>!['transparent','rgba(0, 0, 0, 0)'].includes(getComputedStyle(document.getElementById('ahd-menu'),'::backdrop').backgroundColor));
 assert.ok(Math.abs(box.width-304)<1&&Math.abs(box.x+box.width-1280)<1);
 assert.ok(await drawer.evaluate(e=>e.matches(':modal')&&e.contains(document.activeElement)));
 assert.ok(await drawer.evaluate(e=>!['transparent','rgba(0, 0, 0, 0)'].includes(getComputedStyle(e,'::backdrop').backgroundColor)),'modal backdrop is present');
 const after=await page.locator('main.container').boundingBox();
 assert.deepEqual({x:after.x,y:after.y,width:after.width},{x:before.x,y:before.y,width:before.width},'drawer does not shift or resize page content');
 await page.keyboard.press('Tab');assert.ok(await drawer.evaluate(e=>e.contains(document.activeElement)));
 await page.keyboard.press('Escape');await page.locator('#ahd-menu[open]').waitFor({state:'hidden'});
 assert.ok(await page.locator('#ahd-menu-trigger').evaluate(e=>e===document.activeElement));
 await openMenu();await page.mouse.click(10,100);await page.locator('#ahd-menu[open]').waitFor({state:'hidden'});
 await openMenu();
 const darkToken=await page.locator('html').evaluate(e=>getComputedStyle(e).getPropertyValue('--canvas'));
 assert.equal(await drawer.getByRole('combobox',{name:'Theme',exact:true}).inputValue(),'system','a first visit follows the system color scheme');
 await drawer.getByRole('combobox',{name:'Theme',exact:true}).selectOption('light');
 await page.waitForFunction(()=>document.documentElement.dataset.theme==='light'&&document.getElementById('theme-color')?.content==='#F5F4F8');
 assert.notEqual(await page.locator('html').evaluate(e=>getComputedStyle(e).getPropertyValue('--canvas')),darkToken);
 await drawer.getByRole('combobox',{name:'Language',exact:true}).selectOption('ja');
 await page.waitForFunction(()=>document.documentElement.lang==='ja'&&localStorage.getItem('ahd-language')==='ja');
 await drawer.getByRole('combobox',{name:'テーマ',exact:true}).waitFor();
 assert.equal(await drawer.getAttribute('aria-label'),'アカウントメニュー');
 assert.equal(await page.locator('#ahd-menu-trigger').getAttribute('aria-label'),'メニューを開く');
 assert.equal(await drawer.getByRole('combobox',{name:'言語',exact:true}).inputValue(),'ja');
 await page.locator('h1').getByText('ダッシュボード',{exact:true}).waitFor();
 assert.equal(await genshin.locator('time.estimate-at').innerText(),'10/19 17:38 JST','compact dates stay consistent after language changes');
 assert.equal(await page.locator('#selected-genshin').inputValue(),uid(6),'language change preserves selected UID');
 assert.equal(await page.locator('#selected-starrail').inputValue(),uid(8));
 await page.reload();await idle();
 assert.equal(await page.locator('html').getAttribute('data-theme'),'light','theme persists across reload (DOM only)');
 assert.equal(await page.locator('html').getAttribute('lang'),'ja');
 await openMenu();await drawer.getByRole('combobox',{name:'言語',exact:true}).selectOption('en');
 await drawer.getByRole('combobox',{name:'Theme',exact:true}).waitFor();
 assert.equal(await drawer.getAttribute('aria-label'),'Account menu');
 assert.equal(await page.locator('#ahd-menu-trigger').getAttribute('aria-label'),'Open menu');
 assert.equal(await drawer.getByRole('combobox',{name:'Language',exact:true}).inputValue(),'en');
 await drawer.getByRole('combobox',{name:'Theme',exact:true}).selectOption('dark');
 await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark'&&localStorage.getItem('ahd-theme')==='dark');
 await page.keyboard.press('Escape');await dashboard();
 // Mobile ARIA tabs: hidden panel, roving focus and keyboard edges.
 await page.setViewportSize({width:390,height:844});
 await page.waitForFunction(()=>document.getElementById('game-panel-genshin')?.getAttribute('role')==='tabpanel');
 assert.ok(await brandIcon.isVisible());
 assert.ok(await page.locator('.topbar-inner').evaluate(e=>e.scrollWidth<=e.clientWidth),'mobile header fits with icon');
 assert.ok(await genshin.isVisible());assert.ok(await starrail.isHidden());
 await page.locator('#ahd-menu').evaluate(async element=>{await Promise.all(element.getAnimations().map(animation=>animation.finished.catch(()=>{})));});
 await page.screenshot({path:path.join(root,'target/ui-dashboard-mobile.png'),fullPage:true});
 await page.locator('#game-tab-genshin').focus();await page.keyboard.press('ArrowRight');
 await starrail.waitFor();assert.ok(await genshin.isHidden());
 assert.equal(await page.locator('#game-tab-starrail').getAttribute('aria-selected'),'true');
 assert.equal(await page.locator('#game-tab-starrail').getAttribute('aria-controls'),'game-panel-starrail');
 assert.equal(await starrail.getAttribute('aria-labelledby'),'game-tab-starrail');
 assert.equal(await page.locator('#game-tab-genshin').getAttribute('tabindex'),'-1');
 assert.ok(await page.locator('#game-tab-starrail').evaluate(e=>e===document.activeElement));
 await page.keyboard.press('Home');await genshin.waitFor();
 await page.keyboard.press('End');await starrail.waitFor();
 await page.setViewportSize({width:1280,height:900});
 // Pathname navigation supports back/forward and reload without bypassing auth gates.
 await menu('Settings');
 await page.locator('#settings-tab-security').click();await idle();
 await page.goBack();await page.locator('#settings-tab-uids[aria-selected=true]').waitFor();await idle();
 await page.goForward();await page.locator('#settings-tab-security[aria-selected=true]').waitFor();await idle();
 await page.reload();await page.locator('#settings-tab-security[aria-selected=true]').waitFor();await idle();
 // Every settings tab survives a reload, including notifications.
 await page.locator('#settings-tab-notifications').click();await idle();
 await page.waitForURL(origin+'/settings/notifications');
 await page.reload();await page.locator('#settings-tab-notifications[aria-selected=true]').waitFor();await idle();
 await page.locator('#webhook-url').waitFor();
 await page.locator('a.wordmark').click();await dashboard();
 assert.equal(new URL(page.url()).pathname,'/dashboard');
 assert.equal(new URL(page.url()).hash,'');
 await page.goto(origin+'/settings');
 await page.locator('#settings-tab-uids[aria-selected=true]').waitFor();await idle();
 await page.waitForURL(origin+'/settings/games');
 await page.locator('a.wordmark').click();await dashboard();
 await page.goto(origin+'/login');await dashboard();
 // Public docs skip /me; leaving them checks the session instead of assuming sign-out.
 const apiTitle=JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')).info.title;
 const meBeforeDocs=counts.get('/api/v1/me');
 await page.goto(origin+'/docs');await page.getByRole('heading',{name:apiTitle,exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/me'),meBeforeDocs,'docs entry makes no session request');
 await page.locator('a.wordmark').click();await dashboard();
 assert.equal(await page.locator('#login-username').count(),0,'a valid session survives leaving docs');
 assert.equal(counts.get('/api/v1/me'),meBeforeDocs+1);
 const docsReads=counts.get('/openapi.json');
 stage='settings';
 await settings('link');
 const syncsBefore=counts.get('/api/v1/me/hoyolab/sync')||0;
 await button('Sync UIDs').click();await idle();
 await page.locator('.status.success').getByText('UIDs synced. Choose new UIDs to show under Game display.',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/me/hoyolab/sync'),syncsBefore+1);
 assert.equal(await page.title(),'Link – AHD','the tab names the page');
 // Removing the Cookie asks first; cancelling sends nothing.
 await button('Remove Cookie').click();
 const removeDialog=page.locator('#remove-cookie-dialog');
 await removeDialog.waitFor();
 assert.ok((await removeDialog.innerText()).includes('automatic claiming turns OFF'));
 await removeDialog.getByRole('button',{name:'Cancel',exact:true}).click();
 await removeDialog.waitFor({state:'detached'});
 assert.equal(counts.get('/api/v1/me/hoyolab/cookie'),1,'cancelled removal sends no request');
 await page.locator('#settings-tab-link[aria-selected=true]').waitFor();
 await page.locator('#settings-tab-uids').click();await idle();
 const visibleBox=page.locator('#account-visible-'+uid(5));
 assert.ok(await visibleBox.isChecked());
 failPatch=true;await visibleBox.click();await idle();
 await page.getByText('This feature is not available right now.',{exact:true}).waitFor();
 assert.ok(await visibleBox.isChecked(),'failed visibility PATCH resets the checkbox');
 failPatch=false;
 // Settings replaces the removed nav/schedule checkbox/input[type=time].
 await settings('bonus');
 assert.equal(await page.locator('.settings-tabs [role=tab]').count(),6);
 assert.equal(await page.locator('#settings-panel').getAttribute('aria-labelledby'),'settings-tab-bonus');
 assert.equal(await page.locator('#settings-panel input[type=time]').count(),0);
 assert.equal(await page.locator('#settings-panel input[role=switch]').count(),2,'each game has its automatic claiming switch here too');
 assert.ok(await button('Save time').first().isDisabled(),'nothing to save before a change');
 assert.equal(await page.locator('#settings-panel hr.divider').count(),1,'history is separated from bonus settings');
 assert.equal(await page.locator('#reward-hour-'+uid(20)+' option').count(),24);
 assert.equal(await page.locator('#reward-minute-'+uid(20)+' option').count(),60);
 assert.ok((await page.locator('.timezone').first().innerText()).includes('+09:00'));
 await page.locator('#reward-hour-'+uid(20)).selectOption('07');
 await page.locator('#reward-minute-'+uid(20)).selectOption('13');
 await page.locator('#reward-unsaved-'+uid(20)).getByText('The time change is not saved yet.',{exact:true}).waitFor();
 await button('Save time').first().click();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(20),enabled:true,local_time:'07:13'});
 assert.equal(await page.locator('#reward-unsaved-'+uid(20)).count(),0,'saving clears the unsaved note');
 await page.locator('#reward-minute-'+uid(21)).selectOption('05');
 await button('Save time').nth(1).click();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(21),enabled:false,local_time:'06:05'},'schedule preserves OFF too');
 // Discarding returns to the stored time without a request.
 const patchesBeforeDiscard=rewardPatches.length;
 await page.locator('#reward-hour-'+uid(21)).selectOption('09');
 await button('Discard change').click();
 assert.equal(await page.locator('#reward-hour-'+uid(21)).inputValue(),'06');
 assert.equal(rewardPatches.length,patchesBeforeDiscard);
 assert.equal(await page.locator('#reward-day-note').innerText(),'The login bonus day changes at 01:00 server time. A time between 0:00 and 01:00 claims the previous day\'s bonus.');
 // Settings show compact local times and localized errors, never raw ISO strings or codes.
 const iso=/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/;
 const bonusText=await page.locator('#settings-panel').innerText();
 assert.doesNotMatch(bonusText,iso);assert.match(bonusText,/\d{2}\/\d{2} \d{2}:\d{2} JST/);
 assert.ok(bonusText.includes('HoYoLAB is temporarily unavailable')&&!bonusText.includes('http:503'),'run history maps error codes');
 assert.equal(await button('Run now').count(),0,'manual claiming lives in one dashboard control');
 await page.locator('#history-game').selectOption('starrail');
 await page.getByText('No runs returned for the last 5 days.',{exact:true}).waitFor();
 await page.locator('#history-game').selectOption('');
 assert.equal(await page.locator('#reward-run-state-'+uid(20)).innerText(),'Last run: Never');
 // One dashboard control claims every game still open today, then follows by GET only.
 await page.locator('a.wordmark').click();await dashboard();
 const claim=button('Claim login bonuses now');
 assert.equal(await page.title(),'Dashboard – AHD');
 assert.equal(await page.locator('a.skip-link').getAttribute('href'),'#main');
 assert.equal(await genshin.locator('.award').innerText(),'Next reward: Primogem ×20');
 assert.match(await genshin.locator('.bonus-today').innerText(),/^Today: Not received · Day 12 this month$/);
 await page.waitForLoadState('networkidle');
 const settingsReads=counts.get('/api/v1/reward-settings');
 await claim.click();await idle();
 await page.getByText('Run accepted; queued, not completed.',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/reward-settings/'+uid(21)+'/run')||0,0,'an already claimed game is not run');
 await page.locator('.claim-now').getByText('Checking automatically…',{exact:true}).waitFor();
 assert.ok(await claim.isDisabled());
 await page.locator('.live-region').getByText('Manual run finished: the login bonus was claimed.',{exact:true}).waitFor();
 // One read after the POST and two polls; the dashboard's minute refresh may add one more.
 const pollReads=counts.get('/api/v1/reward-settings')-settingsReads;
 assert.ok(pollReads>=3&&pollReads<=4,'one read after the POST, then two polls');
 assert.equal(counts.get('/api/v1/reward-settings/'+uid(20)+'/run'),1,'polling never re-sends the run');
 // Leaving the page stops a poll for a run that does not settle.
 runPlan=['queued'];
 await claim.click();await idle();
 await page.locator('.claim-now').getByText('Checking automatically…',{exact:true}).waitFor();
 await settings('link');
 const readsAfterLeave=counts.get('/api/v1/reward-settings');
 await page.waitForTimeout(4000);
 assert.equal(counts.get('/api/v1/reward-settings'),readsAfterLeave,'no polling after leaving the dashboard');
 assert.doesNotMatch(await page.locator('#settings-panel').innerText(),iso,'HoYoLAB link times are formatted');
 await page.locator('#settings-tab-uids').click();await idle();
 assert.doesNotMatch(await page.locator('#settings-panel').innerText(),iso,'UID detection times are formatted');
 assert.equal(counts.get('/api/v1/reward-settings/'+uid(20)+'/run'),2);
 rewardSettings[0].last_run={id:uid(30),state:'unknown',ended_at:null};
 await page.locator('a.wordmark').click();await dashboard();
 await page.locator('#claim-now-hint').getByText('The last outcome is unconfirmed, so it cannot run again. Check the run history.',{exact:true}).waitFor();
 assert.ok(await claim.isDisabled(),'unknown acknowledgement is not safe to retry');
 assert.equal(await claim.getAttribute('aria-describedby'),'claim-now-hint');
 await settings('bonus');
 await page.locator('#settings-tab-bonus').focus();await page.keyboard.press('End');
 await page.locator('#delete-password').waitFor();
 const consequences=await page.locator('#delete-consequences').innerText();
 assert.ok(consequences.includes('This cannot be undone')&&consequences.includes('signed out'),'deletion explains irreversibility and sign-out');
 await eyes(['delete-password']);
 await page.locator('#delete-confirmation').fill(user.username+' ');
 await page.locator('#delete-password').fill('wrong-synthetic-password');
 assert.ok(await button('Delete user').isDisabled());
 await page.locator('#delete-confirmation').fill(user.username);
 await button('Delete user').click();await idle();
 await page.getByText('The current password was not accepted. Please try again.',{exact:true}).waitFor();
 assert.equal(await page.locator('#login-username').count(),0,'wrong confirmation password probes and preserves session');
 await page.locator('#settings-tab-delete').focus();await page.keyboard.press('Home');
 await page.locator('#settings-tab-uids[aria-selected=true]').waitFor();
 await page.locator('#settings-tab-uids').focus();await page.keyboard.press('ArrowRight');
 await page.locator('#settings-tab-bonus[aria-selected=true]').waitFor();
 await page.locator('#settings-tab-security').click();await idle();
 await eyes(['current-password','new-password','confirm-password']);
 await change('wrong-synthetic-password');
 await page.getByText('The current password was not accepted. Please try again.',{exact:true}).waitFor();
 assert.equal(await page.locator('#login-username').count(),0);
 // A client-side failure must be retranslated in place, without retrying the mutation.
 const passwordRequestsBeforeLanguage=counts.get('/api/v1/me/password');
 const passwordValidation='Use a matching new password of 12–256 bytes. Ordinary changes also require your current password.';
 await button('Change password').click();await idle();
 await page.locator('.status.error').getByText(passwordValidation,{exact:true}).waitFor();
 assert.equal(await page.locator('#current-password-error').innerText(),'Enter your current password.');
 assert.equal(await page.locator('#new-password-error').innerText(),'Enter a new password.');
 assert.ok(await page.locator('#current-password').evaluate(e=>e===document.activeElement),'focus moves to the first problem');
 // Preferences remask every mounted security field; navigation unmount clears them.
 for(const id of ['current-password','new-password','confirm-password']){
  await page.locator('#'+id).fill('synthetic-navigation-secret');
  await page.locator('button[aria-controls="'+id+'"]').click();
 }
 await openMenu();await drawer.getByRole('combobox',{name:'Language',exact:true}).selectOption('ja');
 await page.waitForFunction(()=>['current-password','new-password','confirm-password'].every(id=>document.getElementById(id)?.type==='password'));
 await page.locator('.status.error').getByText('新しいパスワードは12〜256バイトで確認欄と一致する必要があります。通常の変更には現在のパスワードも必要です。',{exact:true}).waitFor();
 assert.equal(await page.locator('.live-region').getAttribute('aria-live'),'polite');
 assert.equal(await page.locator('#settings-tab-security').getAttribute('aria-label'),'セキュリティ');
 await page.setViewportSize({width:375,height:812});
 assert.equal(await page.locator('#settings-tab-security').innerText(),'セキュリティ','the tab keeps its name at mobile width');
 assert.ok(await page.locator('.settings-tabs').evaluate(e=>e.scrollWidth<=e.clientWidth),'mobile settings tabs fit');
 await page.setViewportSize({width:1280,height:900});
 assert.equal(await page.locator('#settings-tab-security').getAttribute('aria-selected'),'true');
 assert.equal(await page.locator('#settings-panel').getAttribute('aria-labelledby'),'settings-tab-security');
 assert.equal(await page.locator('button[aria-controls="current-password"]').getAttribute('aria-label'),'パスワードを表示');
 await drawer.getByRole('combobox',{name:'言語',exact:true}).selectOption('en');await page.keyboard.press('Escape');
 await page.locator('.status.error').getByText(passwordValidation,{exact:true}).waitFor();
 assert.equal(await page.locator('#settings-tab-security').getAttribute('aria-label'),'Security');
 assert.equal(await page.locator('button[aria-controls="current-password"]').getAttribute('aria-label'),'Show password');
 assert.equal(counts.get('/api/v1/me/password'),passwordRequestsBeforeLanguage,'validation and language changes never resubmit passwords');
 for(const id of ['current-password','new-password','confirm-password'])assert.ok(await page.locator('#'+id).evaluate(e=>e.value!==''&&e.type==='password'),'language changes preserve input without revealing it');
 await page.locator('#settings-tab-uids').click();await page.locator('#settings-tab-security').click();await idle();
 for(const id of ['current-password','new-password','confirm-password'])assert.ok(await page.locator('#'+id).evaluate(e=>e.value===''&&e.type==='password'));
 const cdp=await context.newCDPSession(page);await cdp.send('WebAuthn.enable');
 await cdp.send('WebAuthn.addVirtualAuthenticator',{options:{protocol:'ctap2',transport:'internal',hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}});
 await page.locator('#passkey-label').fill('Synthetic browser key');await button('Register passkey').click();await idle();
 await page.getByText('Synthetic browser key',{exact:true}).waitFor();
 // A page load still in flight when an action starts must land: actions reserve
 // the UI without invalidating the session's loads.
 await page.goto(origin+'/settings/games');await page.locator('#settings-tab-uids[aria-selected=true]').waitFor();await idle();
 passkeyHold=new Promise(resolve=>{releasePasskeys=resolve;});
 const passkeyRead=page.waitForRequest(origin+'/api/v1/me/passkeys');
 await page.locator('#settings-tab-security').click();await passkeyRead;
 await page.getByText('Loading passkeys…',{exact:true}).waitFor();
 assert.equal(await page.getByText('No passkeys registered.',{exact:true}).count(),0,'an unloaded list is not reported as empty');
 await change('wrong-synthetic-password');
 await page.getByText('The current password was not accepted. Please try again.',{exact:true}).waitFor();
 releasePasskeys();passkeyHold=null;
 await page.getByText('Synthetic browser key',{exact:true}).waitFor();
 assert.doesNotMatch(await page.locator('#settings-panel').innerText(),/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/,'passkey dates are formatted');
 assert.ok((await page.locator('#settings-panel').innerText()).includes('Last used Never'));
 // Renaming goes through a dialog and keeps the key.
 await page.getByRole('button',{name:'Rename passkey “Synthetic browser key”',exact:true}).click();
 await page.locator('#passkey-rename-input').fill('Desk laptop');
 await page.locator('#passkey-rename-dialog').getByRole('button',{name:'Rename',exact:true}).click();await idle();
 await page.locator('.status.success').getByText('Passkey renamed.',{exact:true}).waitFor();
 await page.getByText('Desk laptop',{exact:true}).waitFor();
 passkeys[0].label='Synthetic browser key';
 await page.locator('#settings-tab-uids').click();await page.locator('#settings-tab-security').click();await idle();
 await page.getByText('Synthetic browser key',{exact:true}).waitFor();
 // Sessions: this browser is marked; another can be signed out alone, or all others at once.
 const sessionList=page.locator('.sessions');
 await sessionList.getByText('Chrome · Windows',{exact:true}).waitFor();
 assert.equal(await sessionList.locator('li').count(),3);
 assert.ok((await sessionList.locator('li').first().innerText()).includes('This browser'));
 assert.equal(await sessionList.locator('li').first().getByRole('button').count(),0,'the current session signs out from the menu');
 await page.getByRole('button',{name:'Sign out the Safari · iOS session',exact:true}).click();await idle();
 await page.locator('.status.success').getByText('The session is signed out.',{exact:true}).waitFor();
 assert.equal(await sessionList.locator('li').count(),2);
 await button('Sign out all other sessions').click();
 assert.ok((await page.locator('#sessions-revoke-dialog').innerText()).includes('1 session(s)'));
 await page.locator('#sessions-revoke-dialog').getByRole('button',{name:'Sign out all other sessions',exact:true}).click();await idle();
 await page.locator('.status.success').getByText('Other sessions are signed out.',{exact:true}).waitFor();
 assert.equal(await sessionList.locator('li').count(),1);
 assert.equal(await button('Sign out all other sessions').count(),0);
 // Deleting a passkey needs confirmation; Cancel and Escape keep it and return focus.
 const deletePasskey=page.getByRole('button',{name:'Delete passkey “Synthetic browser key”',exact:true});
 const passkeyDialog=page.locator('#passkey-delete-dialog');
 await deletePasskey.click();await page.locator('#passkey-delete-dialog[open]').waitFor();
 assert.ok((await passkeyDialog.innerText()).includes('You will no longer be able to sign in with it.'));
 assert.ok(await passkeyDialog.evaluate(e=>e.matches(':modal')));
 await passkeyDialog.getByRole('button',{name:'Cancel',exact:true}).click();
 await page.locator('#passkey-delete-dialog').waitFor({state:'detached'});
 assert.ok(await deletePasskey.evaluate(e=>e===document.activeElement),'focus returns to the delete button');
 await deletePasskey.click();await page.locator('#passkey-delete-dialog[open]').waitFor();
 await page.keyboard.press('Escape');await page.locator('#passkey-delete-dialog').waitFor({state:'detached'});
 assert.equal(passkeyDeletes,0,'cancelled confirmations send nothing');
 await page.getByText('Synthetic browser key',{exact:true}).waitFor();
 await deletePasskey.click();
 await passkeyDialog.getByRole('button',{name:'Delete',exact:true}).click();await idle();
 await page.locator('.status.success').getByText('Passkey deleted.',{exact:true}).waitFor();
 await page.getByText('No passkeys registered.',{exact:true}).waitFor();
 assert.equal(passkeyDeletes,1);
 stage='admin';
 // Admin generated credentials: entropy failure and API failure never reveal a candidate.
 await menu('Administration');
 assert.equal(await button('Load logs').count(),0,'no permanently disabled log section without an endpoint');
 assert.equal(await page.getByRole('heading',{name:'Logs',exact:true}).count(),0);
 assert.equal(await page.locator('[data-slot=card]').first().evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(24, 23, 29)','AHD card skin does not depend on hashed module class names');
 assert.equal(await page.locator('input[autocomplete=new-password]').count(),0,'no administrator-entered temporary password form');
 const ownRow=page.getByRole('row').filter({has:page.getByRole('rowheader',{name:new RegExp('^'+user.username)})});
 // Linkage health, last sign-in and the audit log are visible; search narrows the table.
 const otherRowEarly=page.getByRole('row').filter({has:page.getByRole('rowheader',{name:new RegExp('^'+other.username)})});
 assert.ok((await otherRowEarly.innerText()).includes('Verified')&&(await otherRowEarly.innerText()).includes('Auto-claim 2'));
 assert.ok((await otherRowEarly.innerText()).includes('Never'));
 const audit=page.getByRole('table').filter({hasText:'User created'});
 assert.ok((await audit.innerText()).includes('System')&&(await audit.innerText()).includes('Deleted user'));
 await page.locator('#admin-search').fill(other.username);
 assert.equal(await page.locator('.admin-users tbody tr').count(),1);
 await page.locator('#admin-search').fill('');
 assert.equal(await ownRow.getByRole('button',{name:'Remove admin',exact:true}).count()+await ownRow.getByRole('button',{name:'Make admin',exact:true}).count(),1);
 assert.ok(await ownRow.getByRole('button',{name:/admin$/}).isDisabled(),'an administrator cannot change their own role');
 assert.equal(await page.getByRole('table',{name:'Manage users',exact:true}).count(),1);
 const actionColumns=await page.locator('.admin-user-actions').evaluateAll(rows=>rows.map(row=>[...row.querySelectorAll('button')].map(button=>Math.round(button.getBoundingClientRect().x))));
 assert.deepEqual(actionColumns[0],actionColumns[1],'all lifecycle action columns align despite different password labels');
 const buttonHeights=await page.locator('.admin-user-actions button').evaluateAll(buttons=>buttons.map(button=>Math.round(button.getBoundingClientRect().height)));
 assert.equal(new Set(buttonHeights).size,1,'action labels do not wrap and change row button heights');
 assert.equal(await page.getByText('The last active administrator cannot be disabled or deleted.',{exact:true}).count(),0,'protection warning is not permanent UI');
 assert.ok(await ownRow.getByRole('button',{name:'Disable',exact:true}).isDisabled());
 assert.ok(await ownRow.getByRole('button',{name:'Delete user',exact:true}).isDisabled());
 // Disabled actions drop the fill and use a dashed outline in both themes.
 const peerRow=page.getByRole('row').filter({has:page.getByRole('rowheader',{name:new RegExp('^'+other.username)})});
 const look=locator=>locator.evaluate(e=>{const s=getComputedStyle(e);return {background:s.backgroundColor,border:s.borderTopStyle,color:s.color};});
 for(const theme of ['dark','light']){
  await page.evaluate(theme=>document.documentElement.setAttribute('data-theme',theme),theme);
  const disabled=await look(ownRow.getByRole('button',{name:'Delete user',exact:true})),enabled=await look(peerRow.getByRole('button',{name:'Delete user',exact:true}));
  assert.deepEqual([disabled.background,disabled.border],['rgba(0, 0, 0, 0)','dashed'],theme+' disabled delete');
  assert.equal(enabled.border,'solid',theme+' enabled delete');
  assert.notEqual(enabled.background,'rgba(0, 0, 0, 0)',theme+' enabled delete keeps its fill');
  assert.notEqual(disabled.color,enabled.color,theme+' disabled text is muted');
 }
 await page.evaluate(()=>document.documentElement.setAttribute('data-theme','dark'));
 assert.equal(await ownRow.getByRole('button',{name:'Reissue password',exact:true}).count(),0,'self-reissue would revoke the only session able to receive the generated secret');
 await ownRow.getByRole('button',{name:'Change password',exact:true}).click();await page.locator('#current-password').waitFor();
 assert.equal(counts.get('/api/v1/admin/users/'+user.id+'/temporary-password')||0,0,'own password uses the ordinary confirmation flow');
 secondAdmin.active=false;
 await menu('Administration');
 await ownRow.getByRole('button',{name:'Delete user',exact:true}).click();
 await page.getByText('The last active administrator cannot be disabled or deleted.',{exact:true}).waitFor();
 assert.equal(await page.locator('#admin-delete-'+user.id+'[open]').count(),0,'last-admin attempt shows feedback, not a delete dialog');
 assert.equal(counts.get('/api/v1/admin/users/'+user.id)||0,0,'last-admin attempt sends no lifecycle request');
 secondAdmin.active=true;
 await menu('Settings');await menu('Administration');
 await page.screenshot({path:path.join(root,'target/ui-admin-desktop.png'),fullPage:true});
 await page.setViewportSize({width:390,height:844});
 assert.ok(await page.locator('main.container').evaluate(e=>e.scrollWidth<=e.clientWidth),'admin table scrolls within its wrapper on mobile');
 const tableFit=await page.locator('.admin-users').evaluate(wrap=>{const left=wrap.getBoundingClientRect().left;return {scrolls:wrap.scrollWidth>wrap.clientWidth,clipped:[...wrap.querySelectorAll('button')].filter(b=>{const r=b.getBoundingClientRect();return r.left-left<0||r.right-left>wrap.scrollWidth||b.scrollWidth>b.clientWidth;}).length,names:[...wrap.querySelectorAll('tbody th')].map(th=>th.getBoundingClientRect().width/parseFloat(getComputedStyle(th).fontSize))};});
 assert.ok(tableFit.scrolls,'narrow admin table scrolls horizontally');
 assert.equal(tableFit.clipped,0,'admin actions lie fully inside the scrollable width');
 assert.ok(tableFit.names.every(width=>width>=8),'user names keep a readable column instead of one character per line');
 await page.screenshot({path:path.join(root,'target/ui-admin-mobile.png'),fullPage:true});
 await page.setViewportSize({width:1280,height:900});
 await page.locator('#admin-create-username').fill('synthetic-created');
 const adminBefore=counts.get('/api/v1/admin/users')||0;
 await page.evaluate(()=>{window.originalRandom=Crypto.prototype.getRandomValues;window.entropyCalls=0;Crypto.prototype.getRandomValues=function(){window.entropyCalls++;throw new Error('synthetic entropy unavailable');};});
 await button('Create user').click();await idle();
 await page.getByText('Could not generate a secure password. Try again.',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/admin/users')||0,adminBefore);
 assert.equal(await page.locator('#admin-generated-password').count(),0);
 await page.evaluate(()=>{Crypto.prototype.getRandomValues=function(array){window.entropyCalls++;return window.originalRandom.call(this,array);};});
 failCreate=true;await button('Create user').click();await idle();
 await page.getByText('Could not create user.',{exact:true}).waitFor();
 assert.equal(await page.locator('#admin-generated-password').count(),0);
 failCreate=false;await button('Create user').click();await idle();
 await page.locator('#admin-credential-result[open]').waitFor();
 assert.ok(await page.locator('#admin-generated-password').evaluate(e=>/^[A-Za-z0-9_-]{24}$/.test(e.textContent)),'generated result length/alphabet');
 assert.ok(await page.evaluate(()=>window.entropyCalls>=3),'WebCrypto used on create');
 await page.evaluate(()=>{window.originalClipboardWrite=Clipboard.prototype.writeText;Clipboard.prototype.writeText=()=>Promise.reject(new Error('synthetic denied'));});
 await button('Copy').click();await page.getByText('Could not copy. Try again.',{exact:true}).waitFor();
 assert.equal(await page.getByText('Copied',{exact:true}).count(),0);
 await page.evaluate(()=>{Clipboard.prototype.writeText=()=>new Promise(resolve=>{window.finishCopy=resolve;});});
 await button('Copy').click();await page.getByText('Copying…',{exact:true}).waitFor();
 assert.equal(await page.getByText('Copied',{exact:true}).count(),0,'success waits for clipboard promise');
 await page.evaluate(()=>window.finishCopy());await page.getByText('Copied',{exact:true}).waitFor();
 await page.evaluate(()=>{Clipboard.prototype.writeText=window.originalClipboardWrite;});
 await button('Close').click();assert.equal(await page.locator('#admin-generated-password').count(),0);
 const createdRow=page.getByRole('row').filter({has:page.getByRole('rowheader',{name:/^synthetic-created/})});
 await createdRow.getByRole('button',{name:'Reissue password',exact:true}).click();
 // Reissuing invalidates the user's credentials, so it asks first; cancelling sends nothing.
 const reissueDialog=page.locator('#admin-confirm-'+uid(12));
 await reissueDialog.waitFor();
 assert.ok((await reissueDialog.innerText()).includes('passkeys and sessions stop working'));
 await reissueDialog.getByRole('button',{name:'Cancel',exact:true}).click();await reissueDialog.waitFor({state:'detached'});
 assert.equal(counts.get('/api/v1/admin/users/'+uid(12)+'/temporary-password')||0,0);
 await createdRow.getByRole('button',{name:'Reissue password',exact:true}).click();
 await reissueDialog.getByRole('button',{name:'Reissue password',exact:true}).click();await idle();
 await page.locator('#admin-credential-result[open]').waitFor();
 assert.equal(counts.get('/api/v1/admin/users/'+uid(12)+'/temporary-password'),1);
 assert.equal(await page.getByText('Copied',{exact:true}).count(),0,'copy state cannot leak between generated results');
 assert.ok(await page.locator('#admin-generated-password').evaluate(e=>/^[A-Za-z0-9_-]{24}$/.test(e.textContent)));
 await page.keyboard.press('Escape');assert.equal(await page.locator('#admin-generated-password').count(),0);
 const otherRow=page.getByRole('row').filter({has:page.getByRole('rowheader',{name:other.username,exact:true})});
 // Busy-locked controls keep their look; only unavailable ones use the dashed outline.
 adminPatchHold=new Promise(resolve=>{releaseAdminPatch=resolve;});
 await otherRow.getByRole('button',{name:'Re-enable',exact:true}).click();
 const enableDialog=page.locator('#admin-confirm-'+other.id);
 assert.ok((await enableDialog.innerText()).includes('Automatic claiming turns OFF'));
 await enableDialog.getByRole('button',{name:'Re-enable',exact:true}).click();
 await page.locator('fieldset.app-shell[aria-busy=true]').waitFor();
 const lockedDelete=otherRow.getByRole('button',{name:'Delete user',exact:true});
 assert.ok(await lockedDelete.isDisabled(),'busy shell locks every action');
 assert.equal(await lockedDelete.getAttribute('disabled'),null,'busy lock comes from the fieldset, not the button');
 const busyLook=await look(lockedDelete);
 assert.equal(busyLook.border,'solid','busy-locked action is not styled as unavailable');
 assert.notEqual(busyLook.background,'rgba(0, 0, 0, 0)','busy-locked action keeps its fill');
 assert.equal(await lockedDelete.evaluate(e=>getComputedStyle(e).cursor),'progress');
 assert.equal((await look(ownRow.getByRole('button',{name:'Delete user',exact:true}))).border,'dashed','unavailable action stays dashed while busy');
 releaseAdminPatch();adminPatchHold=null;await idle();
 await page.getByText('User re-enabled. Old sessions stay signed out, and automatic claiming is off until the user turns it on again in Settings.',{exact:true}).waitFor();
 await otherRow.getByRole('button',{name:'Delete user',exact:true}).click();
 const deletion=page.locator('#admin-delete-'+other.id);
 await deletion.waitFor();await eyes(['admin-delete-password-'+other.id]);
 await deletion.getByLabel('Your current password',{exact:true}).fill('synthetic-password-123');
 await deletion.getByLabel('Exact target username',{exact:true}).fill(other.username+' ');
 assert.ok(await deletion.getByRole('button',{name:'Delete user',exact:true}).isDisabled());
 await deletion.getByLabel('Exact target username',{exact:true}).fill(other.username);
 await deletion.getByRole('button',{name:'Delete user',exact:true}).click();await idle();
 await page.getByText('User deletion accepted; completion is not confirmed.',{exact:true}).waitFor();
 assert.equal(adminDeletes,1);
 assert.ok(await otherRow.isVisible(),'202 is not proof the target is gone');
 stage='session lifecycle';
 // Existing failed logout/retry/reload and real WebAuthn assertion boundary.
 failLogout=true;await menu('Sign out');await button('Retry server logout').waitFor();
 await page.getByText(/server logout is unconfirmed/).waitFor();
 assert.equal(await page.locator('#game-panel-genshin').count(),0);
 assert.ok((await context.cookies()).some(c=>c.name==='attos_session'&&c.value==='synthetic-a'));
 await page.waitForURL(origin+'/login');
 await page.reload();await dashboard();
 await menu('Sign out');await button('Retry server logout').waitFor();failLogout=false;
 await button('Retry server logout').click();await button('Sign in').waitFor();
 assert.equal((await context.cookies()).filter(c=>c.name==='attos_session').length,0);
 await page.locator('#login-username').fill(user.username);await page.locator('#login-method-passkey').check();
 await button('Use passkey').click();await dashboard();
 assert.equal(counts.get('/api/v1/auth/passkey/options'),1);assert.equal(counts.get('/api/v1/auth/passkey/verify'),1);
 // Reissued, verified/readable account goes straight from mandatory change to ready.
 await menu('Sign out');user.must_change_password=true;
 await signIn(user);await page.locator('#new-password').waitFor();await idle();
 assert.equal(await page.locator('#current-password').count(),0);
 const verifyBefore=counts.get('/api/v1/me/hoyolab/verify');
 await change();await dashboard();
 assert.equal(counts.get('/api/v1/me/hoyolab/verify'),verifyBefore,'verified linkage needs no fabricated reissue gate');
 await menu('Sign out');await button('Sign in').waitFor();await signIn(other);await dashboard();
 await genshin.locator('.resource strong').getByText('77',{exact:true}).waitFor();
 assert.ok((await context.cookies()).some(c=>c.name==='attos_session'&&c.value==='synthetic-b'));
 await openMenu();assert.equal(await drawer.getByRole('button',{name:'Administration',exact:true}).count(),0);await page.keyboard.press('Escape');
 await settings('security');expirePasswordSession=true;
 const passwordBefore=counts.get('/api/v1/me/password');await change('synthetic-password-123');
 await button('Sign in').waitFor();assert.equal(counts.get('/api/v1/me/password'),passwordBefore+1);
 await page.getByText('Please sign in again.',{exact:true}).waitFor();
 expirePasswordSession=false;await signIn(other);await dashboard();await menu('Sign out');await button('Sign in').waitFor();
 // Public mobile docs never probe authenticated resources.
 await page.setViewportSize({width:390,height:844});await context.clearCookies();
 const apiBefore=[...counts].filter(([p])=>p.startsWith('/api/v1/'));
 await page.goto(origin+'/docs');
 await page.getByRole('heading',{name:apiTitle,exact:true}).waitFor();
 assert.equal(await page.locator('#login-username').count(),0);assert.equal(counts.get('/openapi.json'),docsReads+1);
 assert.deepEqual([...counts].filter(([p])=>p.startsWith('/api/v1/')),apiBefore);
 stage='docs-i18n';
 await page.getByRole('columnheader',{name:'Path',exact:true}).waitFor();
 await page.evaluate(()=>localStorage.setItem('ahd-language','ja'));await page.reload();
 await page.getByText('APIドキュメント',{exact:true}).waitFor();
 await page.getByRole('columnheader',{name:'パス',exact:true}).waitFor();
 await page.getByRole('columnheader',{name:'メソッド',exact:true}).waitFor();
 assert.equal(await page.locator('html').getAttribute('lang'),'ja');
 assert.equal(counts.get('/openapi.json'),docsReads+2);
 assert.deepEqual([...counts].filter(([p])=>p.startsWith('/api/v1/')),apiBefore,'translated public docs still make no authenticated requests');
 assert.deepEqual(await page.evaluate(()=>Object.keys(localStorage).sort()),['ahd-language','ahd-theme'],'only non-sensitive preferences persist');
 assert.deepEqual(errors,[],'release CSP/Wasm errors');assert.deepEqual(failures,[]);
 console.log('Dioxus production browser regressions passed (synthetic implemented API fixtures; no live identity or backend authorization evidence).');
}catch(error){
 // Report only source location/type, never assertion values or Playwright call logs.
 const location=error.stack?.match(/browser\.mjs:\d+:\d+/)?.[0];
 console.error('Browser check failed:',JSON.stringify({stage,type:error.name,location,errors,failures,requests:Object.fromEntries(counts)}));
 process.exitCode=1;
}finally{allowLogin();allowSettings();releasePasskeys();releaseAdminPatch();if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
