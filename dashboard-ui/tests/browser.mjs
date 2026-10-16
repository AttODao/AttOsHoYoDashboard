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
let verified=false,passkeys=[],credentialId,failLogout=false,allowLogin,allowSettings,expirePasswordSession=false,sessionExpired=false;
const createdUsers=[];
const secondAdmin={...user,id:uid(14),username:'synthetic-second-admin',must_change_password:false};
let failCreate=false,adminDeletes=0,loginFailureStatus=0;
const rewardPatches=[];
const validGenerated=value=>typeof value==='string'&&/^[A-Za-z0-9_-]{24}$/.test(value);
const loginGate=new Promise(resolve=>{allowLogin=resolve;});
const settingsGate=new Promise(resolve=>{allowSettings=resolve;});
const counts=new Map();const failures=[];
const accounts=[{id:uid(3),visible:false,detected:true,game_uid:'600000003'},{id:uid(4),visible:true,detected:false,game_uid:'600000004'},{id:uid(5),visible:true,detected:true,game_uid:'600000005'}].map(a=>({...a,hoyolab_account_id:uid(2),game_kind:'genshin',region:'os_usa',last_detected_at:new Date().toISOString()}));
accounts.push(...[{id:uid(6),game_kind:'genshin',game_uid:'600000006'},{id:uid(7),game_kind:'starrail',game_uid:'700000007'},{id:uid(8),game_kind:'starrail',game_uid:'700000008'}].map(a=>({...accounts[2],...a})));
const rewardSettings=['genshin','starrail'].map((game_kind,i)=>({id:uid(20+i),game_kind,enabled:false,local_time:'06:00',auth_status:'ready',next_run_at:null,last_run:null,is_sign:!!i,last_checked_at:new Date().toISOString(),error_code:null,error_at:null}));
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
   if(p==='/api/v1/me')return json(res,{...actor,onboarding:{password_change_required:actor.must_change_password,cookie_setup_complete:verified,cookie_status:verified?'verified':'pending_verification',verification_available:true},capabilities:{reward_posting_available:true}});
   if(p==='/api/v1/me/hoyolab'){await settingsGate;return json(res,{id:uid(2),hoyolab_uid:verified?'123':null,status:verified?'verified':'pending_verification',setup_complete:verified,updated_at:null,verification_available:true});}
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
   if(p==='/api/v1/me/hoyolab/cookie'){assert.equal(actor.must_change_password,false);assert.ok(body.cookie.startsWith('ltuid_v2=123;'));res.writeHead(202,{'Content-Type':'application/json'});return res.end('ignored acknowledgement');}
   if(p==='/api/v1/me/hoyolab/verify'){assert.equal(req.method,'POST');assert.deepEqual(body,{});verified=true;res.writeHead(204);return res.end();}
   if(p==='/api/v1/me/passkeys/options')return json(res,{challenge_id:uid(10),options:{publicKey:{challenge:Buffer.from('synthetic-browser-challenge-32-bytes').toString('base64url'),rp:{id:'localhost',name:'Synthetic UI check'},user:{id:Buffer.from('synthetic-user').toString('base64url'),name:user.username,displayName:user.username},pubKeyCredParams:[{type:'public-key',alg:-7}],timeout:60000,authenticatorSelection:{userVerification:'required',residentKey:'preferred'},attestation:'none'}}});
   if(p==='/api/v1/me/passkeys/verify'){
    assert.ok(body.response.response.clientDataJSON);assert.ok(body.response.response.attestationObject);assert.equal(body.response.type,'public-key');
    assert.equal(typeof body.response.id,'string');assert.match(body.response.id,/^[A-Za-z0-9_-]+$/);
    assert.equal(body.response.id,body.response.rawId,'native credential id matches base64url rawId');
    assert.equal(Buffer.from(body.response.rawId,'base64url').toString('base64url'),body.response.rawId);
    credentialId=body.response.rawId;passkeys=[{id:uid(11),label:'Synthetic browser key',created_at:new Date().toISOString()}];return json(res,{});
   }
   if(p==='/api/v1/me/passkeys')return json(res,passkeys);
   if(p==='/api/v1/server-settings'){await settingsGate;return json(res,{timezone_offset:'+09:00',session_ttl_days:7});}
   if(p==='/api/v1/admin/users'){
    if(req.method==='POST'){assert.ok(actor===user);assert.ok(body.username==='synthetic-created');assert.ok(validGenerated(body.temporary_password),'24-character WebCrypto candidate');if(failCreate)return json(res,{error:'unavailable'},503);const created={...other,id:uid(12),username:body.username,must_change_password:true};createdUsers.push(created);return json(res,created,201);}
    return json(res,[user,other,secondAdmin,...createdUsers]);
   }
   if(p===`/api/v1/admin/users/${uid(12)}/temporary-password`){assert.equal(req.method,'PUT');assert.ok(validGenerated(body.temporary_password));res.writeHead(204);return res.end();}
   if(p===`/api/v1/admin/users/${other.id}`){
    assert.ok(actor===user);
    if(req.method==='PATCH'){assert.deepEqual(body,{active:true});other.active=true;res.writeHead(204);return res.end();}
    assert.equal(req.method,'DELETE');assert.ok(body.confirmation===other.username);assert.ok(body.current_password==='synthetic-password-123');adminDeletes++;res.writeHead(202);return res.end();
   }
   if(p==='/api/v1/game-accounts')return json(res,accounts);
   const account=accounts.find(a=>p===`/api/v1/game-accounts/${a.id}/status`);
   if(account){assert.ok(account.visible&&account.detected);return json(res,{...account,data:account.game_kind==='genshin'?{current_resin:actor===user?(account.id===uid(5)?0:31):77,max_resin:200,finished_task_num:0,total_task_num:4,expeditions:[]}:{current_stamina:account.id===uid(7)?12:24,max_stamina:300},fetched_at:new Date().toISOString(),error_code:null,error_at:null,stale:false});}
   if(p==='/api/v1/reward-settings'){assert.equal(req.method,'GET');return json(res,rewardSettings);}
   const setting=rewardSettings.find(s=>p===`/api/v1/reward-settings/${s.id}`);
   if(setting){assert.equal(req.method,'PATCH');assert.deepEqual(Object.keys(body).sort(),['enabled','local_time']);rewardPatches.push({id:setting.id,...body});Object.assign(setting,body,{next_run_at:new Date(Date.now()+86400000).toISOString()});res.writeHead(204);return res.end();}
   if(p===`/api/v1/reward-settings/${uid(20)}/run`){assert.equal(req.method,'POST');assert.deepEqual(body,{});rewardSettings[0].last_run={id:uid(30),state:'queued',ended_at:null};res.writeHead(202);return res.end();}
   if(p==='/api/v1/reward-runs')return json(res,[]);
   throw new Error('Unexpected implemented-API request: '+req.method+' '+p);
  }
  if(p==='/openapi.json'){assert.equal(req.method,'GET');counts.set(p,(counts.get(p)||0)+1);return json(res,JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')));}
  let file;
  if(p==='/'||p==='/docs'){file=path.join(publicDir,'index.html');res.setHeader('Content-Type','text/html');res.setHeader('Content-Security-Policy',csp);}
  else if(p.startsWith('/assets/')){file=path.resolve(publicDir,'.'+p);assert.ok(file.startsWith(path.join(publicDir,'assets')+path.sep));res.setHeader('Content-Type',({'.wasm':'application/wasm','.js':'text/javascript','.css':'text/css','.png':'image/png'})[path.extname(file)]||'application/octet-stream');}
  else {res.writeHead(404);return res.end();}
  res.end(fs.readFileSync(file));
 }catch{failures.push(`${req.method} ${new URL(req.url,origin).pathname}: fixture assertion failed`);json(res,{error:'fixture_failure'},500);}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));origin=`http://localhost:${server.address().port}`;
let browser,page,stage='login';const errors=[];
try{
 browser=await chromium.launch({headless:true,args:['--no-sandbox']});
 const context=await browser.newContext({viewport:{width:1280,height:900},reducedMotion:process.argv.includes('--normal-motion')?'no-preference':'reduce'});
 context.setDefaultTimeout(10000);
 await context.addInitScript(()=>{if(!localStorage.getItem('agd-language'))localStorage.setItem('agd-language','en');});
 await context.route('**/*',route=>new URL(route.request().url()).origin===origin?route.continue():route.abort());
 page=await context.newPage();
 page.on('pageerror',()=>errors.push('browser page error'));
 page.on('console',message=>{if(message.type()==='error'&&/Content Security Policy|unsafe-eval|inline style/i.test(message.text()))errors.push('CSP violation');});
 const button=name=>page.getByRole('button',{name,exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('fieldset.app-shell')?.disabled===false);
 const dashboard=async()=>{await page.getByRole('heading',{name:'Dashboard',exact:true}).waitFor();await idle();};
 const openMenu=async()=>{await page.locator('#agd-menu-trigger').click();await page.locator('#agd-menu[open]').waitFor();await page.waitForFunction(()=>Math.abs(document.getElementById('agd-menu').getBoundingClientRect().right-innerWidth)<1);};
 const menu=async name=>{await openMenu();await page.locator('#agd-menu').getByRole('button',{name,exact:true}).click();await page.locator('#agd-menu[open]').waitFor({state:'hidden'});await idle();};
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
 await page.locator('.wordmark').getByText('AGD',{exact:true}).waitFor();
 // The supplied central motif is a real transparent asset, not a CSS background/mask.
 const brandIcon=page.locator('.wordmark img');
 assert.equal(await brandIcon.getAttribute('alt'),'','adjacent AGD text names the brand once');
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
 assert.ok(brandBox.x+brandBox.width<brandTextBox.x&&Math.abs(brandBox.y+brandBox.height/2-brandTextBox.y-brandTextBox.height/2)<1,'icon precedes and centers on AGD');
 await page.waitForFunction(()=>document.fonts.size>=2);
 assert.ok(await page.evaluate(async()=>{await document.fonts.ready;return [...document.fonts].filter(f=>f.status==='loaded').length>=2;}),'self-hosted UI and wordmark fonts load');
 assert.equal(await page.getByText('Please sign in.',{exact:true}).count(),0,'no redundant login instructions');
 const usernameBox=await page.locator('#login-username').boundingBox(),passwordBox=await page.locator('#login-password').boundingBox(),submitBox=await button('Sign in').boundingBox();
 assert.ok(usernameBox.height>=48&&passwordBox.height>=48,'default text and password controls share the AGD size');
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
   await page.evaluate(()=>localStorage.setItem('agd-language','ja'));await page.reload();
   await page.locator('#login-username').waitFor();await idle();
  }
  for(const [username,password] of [['synthetic-missing-user','synthetic-password-123'],[user.username,'synthetic-wrong-password']]){
   await page.locator('#login-username').fill(username);await page.locator('#login-password').fill(password);
   await button(language==='ja'?'ログイン':'Sign in').click();
   await page.locator('.status.error').getByText(language==='ja'?'ユーザー名かパスワードが違います':'The username or password is incorrect.',{exact:true}).waitFor();
   await idle();assert.equal(await page.locator('#login-username').count(),1);
  }
 }
 await page.evaluate(()=>localStorage.setItem('agd-language','en'));await page.reload();
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
 assert.equal(await page.locator('#agd-menu-trigger').count(),0,'mandatory setup has no ordinary navigation');
 assert.equal(await button('Sign out').count(),1,'setup permits logout');
 assert.equal(await page.locator('#current-password').count(),0,'mandatory setup never requests a login secret');
 assert.equal(await page.locator('#hoyolab-cookie').count(),0,'password gate hides Cookie actions');
 assert.equal(counts.get('/api/v1/game-accounts')||0,0);
 await eyes(['new-password','confirm-password']);
 // Confirm mismatch makes no request.
 await page.locator('#new-password').fill('synthetic-permanent-password-123');
 await page.locator('#confirm-password').fill('synthetic-mismatch');
 await button('Change password').click();await idle();
 assert.equal(counts.get('/api/v1/me/password')||0,0);
 await change();
 await page.locator('#hoyolab-cookie').waitFor();
 assert.equal(await page.locator('#new-password').count(),0);
 assert.equal(counts.get('/api/v1/me/password'),1);
 assert.equal(counts.get('/api/v1/game-accounts')||0,0,'incomplete real linkage still gates dashboard');
 assert.equal(counts.get('/api/v1/reward-settings')||0,0);
 assert.ok(await button('Sync UIDs').isDisabled());
 await page.locator('#hoyolab-cookie').fill('ltuid_v2=123; ltoken_v2=synthetic-browser-token');
 await button('Save').click();await idle();
 assert.ok(await page.locator('#hoyolab-cookie').evaluate(e=>e.value===''));
 await button('Verify').click();await dashboard();
 assert.equal(counts.get('/api/v1/me/hoyolab/verify'),1);
 stage='dashboard';
 // Dark desktop renders both real game cards, not a screenshot fixture.
 const genshin=page.locator('#game-panel-genshin'),starrail=page.locator('#game-panel-starrail');
 await genshin.locator('.resource strong').getByText('0',{exact:true}).waitFor();
 await starrail.locator('.resource strong').getByText('12',{exact:true}).waitFor();
 assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 assert.equal(await genshin.getAttribute('role'),'region');
 const gbox=await genshin.boundingBox(),sbox=await starrail.boundingBox();
 assert.ok(Math.abs(gbox.y-sbox.y)<2&&sbox.x>gbox.x+gbox.width,'desktop dual columns');
 assert.equal(await page.locator('#selected-genshin option').count(),2);
 assert.equal(await page.locator('#selected-starrail option').count(),2);
 assert.ok(await genshin.locator('.status-metadata').isHidden(),'technical data is collapsed by default');
 await genshin.getByText('Data details',{exact:true}).click();
 assert.ok(await genshin.locator('.status-metadata').isVisible(),'all stored metadata stays available');
 await genshin.getByText('Data details',{exact:true}).click();
 for(const n of [3,4])assert.equal(counts.get('/api/v1/game-accounts/'+uid(n)+'/status')||0,0);
 await page.locator('#selected-genshin').selectOption(uid(6));
 await genshin.locator('.resource strong').getByText('31',{exact:true}).waitFor();
 assert.equal(await page.locator('#selected-starrail').inputValue(),uid(7));
 await page.locator('#selected-starrail').selectOption(uid(8));
 await starrail.locator('.resource strong').getByText('24',{exact:true}).waitFor();
 assert.equal(await page.locator('#selected-genshin').inputValue(),uid(6));
 await genshin.getByRole('switch').check();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(20),enabled:true,local_time:'06:00'});
 assert.equal(rewardSettings[1].enabled,false,'automation PATCH cannot change other game');
 // Drawer is native modal, right aligned, no content shift, keyboard/focus safe.
 await page.evaluate(()=>scrollTo(0,0));
 const before=await page.locator('main.container').boundingBox();
 await openMenu();
 const drawer=page.locator('#agd-menu'),box=await drawer.boundingBox();
 await page.waitForFunction(()=>!['transparent','rgba(0, 0, 0, 0)'].includes(getComputedStyle(document.getElementById('agd-menu'),'::backdrop').backgroundColor));
 assert.ok(Math.abs(box.width-304)<1&&Math.abs(box.x+box.width-1280)<1);
 assert.ok(await drawer.evaluate(e=>e.matches(':modal')&&e.contains(document.activeElement)));
 assert.ok(await drawer.evaluate(e=>!['transparent','rgba(0, 0, 0, 0)'].includes(getComputedStyle(e,'::backdrop').backgroundColor)),'modal backdrop is present');
 const after=await page.locator('main.container').boundingBox();
 assert.deepEqual({x:after.x,y:after.y,width:after.width},{x:before.x,y:before.y,width:before.width},'drawer does not shift or resize page content');
 await page.keyboard.press('Tab');assert.ok(await drawer.evaluate(e=>e.contains(document.activeElement)));
 await page.keyboard.press('Escape');await page.locator('#agd-menu[open]').waitFor({state:'hidden'});
 assert.ok(await page.locator('#agd-menu-trigger').evaluate(e=>e===document.activeElement));
 await openMenu();await page.mouse.click(10,100);await page.locator('#agd-menu[open]').waitFor({state:'hidden'});
 await openMenu();
 const darkToken=await page.locator('html').evaluate(e=>getComputedStyle(e).getPropertyValue('--canvas'));
 await drawer.getByRole('switch',{name:'Dark mode',exact:true}).click();
 await page.waitForFunction(()=>document.documentElement.dataset.theme==='light');
 assert.notEqual(await page.locator('html').evaluate(e=>getComputedStyle(e).getPropertyValue('--canvas')),darkToken);
 await drawer.getByRole('button',{name:'日本語',exact:true}).click();
 await page.waitForFunction(()=>document.documentElement.lang==='ja'&&localStorage.getItem('agd-language')==='ja');
 await drawer.getByRole('switch',{name:'ダークモード',exact:true}).waitFor();
 assert.equal(await drawer.getAttribute('aria-label'),'アカウントメニュー');
 assert.equal(await page.locator('#agd-menu-trigger').getAttribute('aria-label'),'メニューを開く');
 assert.equal(await drawer.getByRole('button',{name:'日本語',exact:true}).getAttribute('aria-pressed'),'true');
 await page.locator('h1').getByText('ダッシュボード',{exact:true}).waitFor();
 assert.equal(await page.locator('#selected-genshin').inputValue(),uid(6),'language change preserves selected UID');
 assert.equal(await page.locator('#selected-starrail').inputValue(),uid(8));
 await page.reload();await idle();
 assert.equal(await page.locator('html').getAttribute('data-theme'),'light','theme persists across reload (DOM only)');
 assert.equal(await page.locator('html').getAttribute('lang'),'ja');
 await openMenu();await drawer.getByRole('button',{name:'English',exact:true}).click();
 await drawer.getByRole('switch',{name:'Dark mode',exact:true}).waitFor();
 assert.equal(await drawer.getAttribute('aria-label'),'Account menu');
 assert.equal(await page.locator('#agd-menu-trigger').getAttribute('aria-label'),'Open menu');
 assert.equal(await drawer.getByRole('button',{name:'English',exact:true}).getAttribute('aria-pressed'),'true');
 await drawer.getByRole('switch',{name:'Dark mode',exact:true}).click();
 await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark'&&localStorage.getItem('agd-theme')==='dark');
 await page.keyboard.press('Escape');await dashboard();
 // Mobile ARIA tabs: hidden panel, roving focus and keyboard edges.
 await page.setViewportSize({width:390,height:844});
 await page.waitForFunction(()=>document.getElementById('game-panel-genshin')?.getAttribute('role')==='tabpanel');
 assert.ok(await brandIcon.isVisible());
 assert.ok(await page.locator('.topbar-inner').evaluate(e=>e.scrollWidth<=e.clientWidth),'mobile header fits with icon');
 assert.ok(await genshin.isVisible());assert.ok(await starrail.isHidden());
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
 stage='settings';
 // Settings replaces the removed nav/schedule checkbox/input[type=time].
 await settings('bonus');
 assert.equal(await page.locator('.settings-tabs [role=tab]').count(),5);
 assert.equal(await page.locator('#settings-panel').getAttribute('aria-labelledby'),'settings-tab-bonus');
 assert.equal(await page.locator('#settings-panel input[type=time],#settings-panel input[type=checkbox]').count(),0);
 assert.equal(await page.locator('#reward-hour-'+uid(20)+' option').count(),24);
 assert.equal(await page.locator('#reward-minute-'+uid(20)+' option').count(),60);
 assert.ok((await page.locator('.timezone').first().innerText()).includes('+09:00'));
 await page.locator('#reward-hour-'+uid(20)).selectOption('07');
 await page.locator('#reward-minute-'+uid(20)).selectOption('13');
 await button('Save').first().click();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(20),enabled:true,local_time:'07:13'});
 await button('Save').nth(1).click();await idle();
 assert.deepEqual(rewardPatches.at(-1),{id:uid(21),enabled:false,local_time:'06:00'},'schedule preserves OFF too');
 await button('Run now').first().click();await idle();
 await page.getByText('Run accepted; queued, not completed.',{exact:true}).waitFor();
 assert.ok(await button('Run now').first().isDisabled());
 assert.equal(counts.get('/api/v1/reward-settings/'+uid(20)+'/run'),1);
 rewardSettings[0].last_run={id:uid(30),state:'unknown',ended_at:null};
 await button('Refresh').click();await idle();
 await page.getByText('Unknown outcome',{exact:true}).waitFor();
 assert.ok(await button('Run now').first().isDisabled(),'unknown acknowledgement is not safe to retry');
 await page.locator('#settings-tab-bonus').focus();await page.keyboard.press('End');
 await page.locator('#delete-password').waitFor();
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
 // Preferences remask every mounted security field; navigation unmount clears them.
 for(const id of ['current-password','new-password','confirm-password']){
  await page.locator('#'+id).fill('synthetic-navigation-secret');
  await page.locator('button[aria-controls="'+id+'"]').click();
 }
 await openMenu();await drawer.getByRole('button',{name:'日本語',exact:true}).click();
 await page.waitForFunction(()=>['current-password','new-password','confirm-password'].every(id=>document.getElementById(id)?.type==='password'));
 await page.locator('.status.error').getByText('新しいパスワードは12〜256バイトで確認欄と一致する必要があります。通常の変更には現在のパスワードも必要です。',{exact:true}).waitFor();
 assert.equal(await page.locator('.live-region').getAttribute('aria-live'),'polite');
 assert.equal(await page.locator('#settings-tab-security').getAttribute('aria-label'),'セキュリティ');
 assert.equal(await page.locator('#settings-tab-security').getAttribute('aria-selected'),'true');
 assert.equal(await page.locator('#settings-panel').getAttribute('aria-labelledby'),'settings-tab-security');
 assert.equal(await page.locator('button[aria-controls="current-password"]').getAttribute('aria-label'),'パスワードを表示');
 await drawer.getByRole('button',{name:'English',exact:true}).click();await page.keyboard.press('Escape');
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
 await page.getByText(/Synthetic browser key — Created/).waitFor();
 stage='admin';
 // Admin generated credentials: entropy failure and API failure never reveal a candidate.
 await menu('Administration');
 assert.ok(await button('Load logs').isDisabled(),'no invented admin log endpoint');
 assert.equal(await page.locator('[data-slot=card]').first().evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(24, 23, 29)','AGD card skin does not depend on hashed module class names');
 assert.equal(await page.locator('input[autocomplete=new-password]').count(),0,'no administrator-entered temporary password form');
 const ownRow=page.locator('.setting-row').filter({has:page.getByRole('heading',{name:user.username,exact:true})});
 assert.ok(await ownRow.getByRole('button',{name:'Disable',exact:true}).isDisabled());
 assert.ok(await ownRow.getByRole('button',{name:'Delete user',exact:true}).isDisabled());
 assert.equal(await ownRow.getByRole('button',{name:'Reissue password',exact:true}).count(),0,'self-reissue would revoke the only session able to receive the generated secret');
 await ownRow.getByRole('button',{name:'Change password',exact:true}).click();await page.locator('#current-password').waitFor();
 assert.equal(counts.get('/api/v1/admin/users/'+user.id+'/temporary-password')||0,0,'own password uses the ordinary confirmation flow');
 await menu('Administration');
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
 const createdRow=page.locator('.setting-row').filter({has:page.getByRole('heading',{name:'synthetic-created',exact:true})});
 await createdRow.getByRole('button',{name:'Reissue password',exact:true}).click();await idle();
 await page.locator('#admin-credential-result[open]').waitFor();
 assert.equal(counts.get('/api/v1/admin/users/'+uid(12)+'/temporary-password'),1);
 assert.equal(await page.getByText('Copied',{exact:true}).count(),0,'copy state cannot leak between generated results');
 assert.ok(await page.locator('#admin-generated-password').evaluate(e=>/^[A-Za-z0-9_-]{24}$/.test(e.textContent)));
 await page.keyboard.press('Escape');assert.equal(await page.locator('#admin-generated-password').count(),0);
 const otherRow=page.locator('.setting-row').filter({has:page.getByRole('heading',{name:other.username,exact:true})});
 await otherRow.getByRole('button',{name:'Re-enable',exact:true}).click();await idle();
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
 await page.getByRole('heading',{name:JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')).info.title,exact:true}).waitFor();
 assert.equal(await page.locator('#login-username').count(),0);assert.equal(counts.get('/openapi.json'),1);
 assert.deepEqual([...counts].filter(([p])=>p.startsWith('/api/v1/')),apiBefore);
 stage='docs-i18n';
 await page.getByRole('columnheader',{name:'Path',exact:true}).waitFor();
 await page.evaluate(()=>localStorage.setItem('agd-language','ja'));await page.reload();
 await page.getByText('APIドキュメント',{exact:true}).waitFor();
 await page.getByRole('columnheader',{name:'パス',exact:true}).waitFor();
 await page.getByRole('columnheader',{name:'メソッド',exact:true}).waitFor();
 assert.equal(await page.locator('html').getAttribute('lang'),'ja');
 assert.equal(counts.get('/openapi.json'),2);
 assert.deepEqual([...counts].filter(([p])=>p.startsWith('/api/v1/')),apiBefore,'translated public docs still make no authenticated requests');
 assert.deepEqual(await page.evaluate(()=>Object.keys(localStorage).sort()),['agd-language','agd-theme'],'only non-sensitive preferences persist');
 assert.deepEqual(errors,[],'release CSP/Wasm errors');assert.deepEqual(failures,[]);
 console.log('Dioxus production browser regressions passed (synthetic implemented API fixtures; no live identity or backend authorization evidence).');
}catch(error){
 // Report only source location/type, never assertion values or Playwright call logs.
 const location=error.stack?.match(/browser\.mjs:\d+:\d+/)?.[0];
 console.error('Browser check failed:',JSON.stringify({stage,type:error.name,location,errors,failures,requests:Object.fromEntries(counts)}));
 process.exitCode=1;
}finally{allowLogin();allowSettings();if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
