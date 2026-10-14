// Compiled-Dioxus UI check only. Synthetic replies on implemented API paths are
// not evidence of backend authorization, upstream identity binding, or real UV.
// Run after dx build; requires Node + Playwright and its Chromium runtime.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.ATTOS_TEST_PLAYWRIGHT_MODULE || 'playwright');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const publicDir=path.join(root,'target/dx/attos-dashboard-ui/release/web/public');
assert.ok(fs.existsSync(path.join(publicDir,'index.html')),'build the Dioxus release UI first');
const csp=fs.readFileSync(path.join(root,'src/dashboard/http/ui.rs'),'utf8').match(/const CSP: &str = "([^"]+)"/)[1];
const uid=n=>`00000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const user={id:uid(1),username:'synthetic-admin',is_admin:true,active:true,must_change_password:true};
const other={...user,id:uid(9),username:'synthetic-other',is_admin:false,must_change_password:false};
let verified=false,passkeys=[],credentialId,failLogout=false,allowLogin,allowSettings;
const createdUsers=[];
const loginGate=new Promise(resolve=>{allowLogin=resolve;});
const settingsGate=new Promise(resolve=>{allowSettings=resolve;});
const counts=new Map();const failures=[];
const accounts=[{id:uid(3),visible:false,detected:true,game_uid:'600000003'},{id:uid(4),visible:true,detected:false,game_uid:'600000004'},{id:uid(5),visible:true,detected:true,game_uid:'600000005'}].map(a=>({...a,hoyolab_account_id:uid(2),game_kind:'genshin',region:'os_usa',last_detected_at:new Date().toISOString()}));
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
   if(p==='/api/v1/auth/login'){const loginUser=body.username===user.username?user:body.username===other.username?other:null;assert.ok(loginUser);await loginGate;res.setHeader('Set-Cookie',`attos_session=synthetic-${loginUser===user?'a':'b'}; HttpOnly; SameSite=Strict; Path=/`);return json(res,loginUser);}
   if(p==='/api/v1/auth/logout'){if(failLogout)return json(res,{error:'unavailable'},503);res.setHeader('Set-Cookie','attos_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0');return json(res,{});}
   if(p==='/api/v1/auth/passkey/options'){assert.equal(body.username,user.username);assert.ok(credentialId);return json(res,{challenge_id:uid(13),options:{publicKey:{challenge:Buffer.from('synthetic-assertion-challenge-32-bytes').toString('base64url'),rpId:'localhost',allowCredentials:[{type:'public-key',id:credentialId,transports:['internal']}],userVerification:'required',timeout:60000}}});}
   if(p==='/api/v1/auth/passkey/verify'){
    assert.equal(body.response.type,'public-key');assert.equal(body.response.id,credentialId);assert.equal(body.response.rawId,credentialId);
    assert.ok(body.response.response.clientDataJSON);assert.ok(body.response.response.authenticatorData);assert.ok(body.response.response.signature);
    assert.ok(body.response.response.userHandle===null||typeof body.response.response.userHandle==='string');
    res.setHeader('Set-Cookie','attos_session=synthetic-a; HttpOnly; SameSite=Strict; Path=/');return json(res,user);
   }
   if(!actor)return json(res,{error:'unauthorized'},401);
   if(p==='/api/v1/me')return json(res,{...actor,onboarding:{password_change_required:actor.must_change_password,cookie_setup_complete:verified,cookie_status:verified?'verified':'pending_verification',verification_available:true},capabilities:{reward_posting_available:true}});
   if(p==='/api/v1/me/hoyolab'){await settingsGate;return json(res,{id:uid(2),hoyolab_uid:verified?'123':null,status:verified?'verified':'pending_verification',setup_complete:verified,updated_at:null,verification_available:true});}
   if(p==='/api/v1/me/password'){assert.equal(req.method,'PUT');assert.deepEqual(body,{current_password:'synthetic-password-123',new_password:'synthetic-permanent-password-123'});actor.must_change_password=false;return json(res,actor);}
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
    if(req.method==='POST'){assert.equal(actor,user);assert.equal(body.username,'synthetic-created');assert.equal(body.temporary_password,'synthetic-temporary-password');createdUsers.push({...other,id:uid(12),username:body.username,must_change_password:true});res.writeHead(201);return res.end();}
    return json(res,[user,{...other,active:false},...createdUsers]);
   }
   if(p==='/api/v1/game-accounts')return json(res,accounts);
   if(p===`/api/v1/game-accounts/${uid(5)}/status`)return json(res,{...accounts[2],data:{current_resin:actor===user?0:77,max_resin:200,finished_task_num:0,total_task_num:4,expeditions:[]},fetched_at:new Date().toISOString(),error_code:null,error_at:null,stale:false});
   if(p==='/api/v1/reward-settings'){assert.equal(req.method,'GET');return json(res,rewardSettings);}
   if(p===`/api/v1/reward-settings/${uid(20)}`){assert.equal(req.method,'PATCH');assert.deepEqual(body,{enabled:true,local_time:'06:00'});Object.assign(rewardSettings[0],body,{next_run_at:new Date(Date.now()+86400000).toISOString()});res.writeHead(204);return res.end();}
   if(p===`/api/v1/reward-settings/${uid(20)}/run`){assert.equal(req.method,'POST');assert.deepEqual(body,{});res.writeHead(202);return res.end();}
   if(p==='/api/v1/reward-runs')return json(res,[]);
   throw new Error('Unexpected implemented-API request: '+req.method+' '+p);
  }
  if(p==='/openapi.json'){assert.equal(req.method,'GET');counts.set(p,(counts.get(p)||0)+1);return json(res,JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')));}
  let file;
  if(p==='/'||p==='/docs'){file=path.join(publicDir,'index.html');res.setHeader('Content-Type','text/html');res.setHeader('Content-Security-Policy',csp);}
  else if(p.startsWith('/assets/')){file=path.resolve(publicDir,'.'+p);assert.ok(file.startsWith(path.join(publicDir,'assets')+path.sep));res.setHeader('Content-Type',({'.wasm':'application/wasm','.js':'text/javascript','.css':'text/css'})[path.extname(file)]||'application/octet-stream');}
  else {res.writeHead(404);return res.end();}
  res.end(fs.readFileSync(file));
 }catch(error){failures.push(error.message);json(res,{error:'fixture_failure'},500);}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));origin=`http://localhost:${server.address().port}`;
let browser,page;const errors=[];
try{
 browser=await chromium.launch({headless:true,args:['--no-sandbox']});
 const context=await browser.newContext();
 await context.route('**/*',route=>new URL(route.request().url()).origin===origin?route.continue():route.abort());
 page=await context.newPage();
 page.on('pageerror',error=>errors.push(error.message));
 page.on('console',message=>{if(message.type()==='error'&&/Content Security Policy|unsafe-eval|inline style/i.test(message.text()))errors.push(message.text());});
 await page.goto(origin);
 await page.getByLabel('Username',{exact:true}).first().fill(user.username);
 await page.getByLabel('Password',{exact:true}).fill('synthetic-password-123');
 const loginRequest=page.waitForRequest(origin+'/api/v1/auth/login');
 await page.getByRole('button',{name:'Sign in',exact:true}).click();await loginRequest;
 assert.ok(await page.getByRole('button',{name:'Sign in',exact:true}).isDisabled());
 assert.ok(await page.getByRole('button',{name:'Use passkey',exact:true}).isDisabled());
 // Bypass native disabled controls to exercise the synchronous reservation too.
 await page.locator('#login-username').evaluate((input,name)=>{input.value=name;input.dispatchEvent(new Event('input',{bubbles:true}));},other.username);
 await page.getByRole('button',{name:'Sign in',exact:true}).dispatchEvent('click');
 await page.getByRole('button',{name:'Use passkey',exact:true}).dispatchEvent('click');
 const metadataRequest=page.waitForRequest(origin+'/api/v1/me/hoyolab');allowLogin();await metadataRequest;
 await page.getByText('Setup status',{exact:true}).waitFor();
 assert.ok(await page.getByRole('button',{name:'Refresh setup status',exact:true}).isDisabled(),'busy survives the bootstrap state reset');
 await page.getByRole('button',{name:'Refresh setup status',exact:true}).dispatchEvent('click');
 allowSettings();
 await page.waitForFunction(()=>document.querySelector('fieldset.app-shell')?.disabled===false);
 await page.getByText('Change your temporary password before saving or verifying a HoYoLAB Cookie.',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/auth/login'),1,'competing password/passkey login was rejected before enqueueing');
 assert.equal(counts.get('/api/v1/auth/passkey/options')||0,0);
 assert.equal(counts.get('/api/v1/me'),2,'bootstrap refresh cannot interrupt login bootstrap');
 assert.equal((await context.cookies()).find(c=>c.name==='attos_session').value,'synthetic-a');
 assert.ok(await page.getByLabel('Paste one HoYoLAB Cookie').isDisabled());
 for(const name of ['Save Cookie','Verify binding','Sync metadata'])assert.ok(await page.getByRole('button',{name,exact:true}).isDisabled());
 assert.equal(counts.get('/api/v1/me/hoyolab/cookie')||0,0);
 await page.getByLabel('Current password',{exact:true}).fill('synthetic-password-123');
 await page.getByLabel('New password',{exact:true}).fill('synthetic-permanent-password-123');
 await page.getByRole('button',{name:'Change password',exact:true}).click();
 await page.getByRole('button',{name:'Re-enable',exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/me/password'),1);
 assert.ok(await page.getByLabel('Paste one HoYoLAB Cookie').isEnabled());
 await page.getByRole('button',{name:'Delete my account',exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/game-accounts')||0,0);
 assert.equal(counts.get('/api/v1/reward-settings')||0,0);
 await page.getByLabel('Paste one HoYoLAB Cookie').fill('ltuid_v2=123; ltoken_v2=synthetic-browser-token');
 await page.getByRole('button',{name:'Save Cookie',exact:true}).click();
 await page.getByText('Setup status',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('Paste one HoYoLAB Cookie').inputValue(),'');
 await page.locator('#admin-create-username').fill('synthetic-created');
 await page.getByLabel('Temporary password',{exact:true}).fill('synthetic-temporary-password');
 await page.getByRole('button',{name:'Create user',exact:true}).click();
 await page.getByText('synthetic-created',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('Temporary password',{exact:true}).inputValue(),'');
 const cdp=await context.newCDPSession(page);await cdp.send('WebAuthn.enable');
 await cdp.send('WebAuthn.addVirtualAuthenticator',{options:{protocol:'ctap2',transport:'internal',hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}});
 await page.getByLabel('New passkey label').fill('Synthetic browser key');
 await page.getByRole('button',{name:'Register passkey',exact:true}).click();
 await page.getByText(/Synthetic browser key — created/).waitFor();
 // Synthetic verification exercises the available=true UI contract, not live identity proof.
 await page.getByRole('button',{name:'Verify binding',exact:true}).click();
 await page.getByRole('heading',{name:'Dashboard',exact:true}).waitFor();
 await page.getByText('0/200',{exact:true}).waitFor();
 assert.equal(await page.locator('#selected-account option').count(),1);
 assert.equal(counts.get(`/api/v1/game-accounts/${uid(3)}/status`)||0,0);
 assert.equal(counts.get(`/api/v1/game-accounts/${uid(4)}/status`)||0,0);
 assert.equal(counts.get('/api/v1/me/hoyolab/verify'),1);
 await page.getByRole('button',{name:'Rewards',exact:true}).click();
 await page.getByRole('heading',{name:'Rewards',exact:true}).waitFor();
 await page.getByRole('button',{name:'Run now',exact:true}).first().waitFor();
 assert.equal(await page.getByText(/Reward posting is not available/).count(),0);
 assert.ok(await page.getByRole('button',{name:'Run now',exact:true}).first().isEnabled());
 await page.getByRole('checkbox').first().check();
 const saveReward=page.waitForResponse(r=>r.url()===origin+`/api/v1/reward-settings/${uid(20)}`&&r.request().method()==='PATCH');
 await page.getByRole('button',{name:'Save',exact:true}).first().click();
 assert.equal((await saveReward).status(),204);
 await page.getByText('Reward setting saved.',{exact:true}).waitFor();
 assert.equal(rewardSettings[0].enabled,true);
 const runReward=page.waitForResponse(r=>r.url()===origin+`/api/v1/reward-settings/${uid(20)}/run`&&r.request().method()==='POST');
 await page.getByRole('button',{name:'Run now',exact:true}).first().click();
 assert.equal((await runReward).status(),202);
 await page.getByText('Manual reward run queued.',{exact:true}).waitFor();
 assert.equal(counts.get(`/api/v1/reward-settings/${uid(20)}/run`),1);
 await page.getByRole('button',{name:'Dashboard',exact:true}).click();
 await page.getByText('0/200',{exact:true}).waitFor();
 failLogout=true;
 await page.getByRole('button',{name:'Logout',exact:true}).click();
 await page.getByRole('button',{name:'Retry server logout',exact:true}).waitFor();
 await page.getByText(/server logout is unconfirmed/).waitFor();
 assert.equal(await page.getByRole('heading',{name:'Dashboard',exact:true}).count(),0);
 assert.equal((await context.cookies()).find(c=>c.name==='attos_session').value,'synthetic-a');
 await page.reload();await page.getByRole('heading',{name:'Dashboard',exact:true}).waitFor();
 await page.getByText(user.username,{exact:true}).waitFor();
 await page.getByRole('button',{name:'Logout',exact:true}).click();
 await page.getByRole('button',{name:'Retry server logout',exact:true}).waitFor();failLogout=false;
 await page.getByRole('button',{name:'Retry server logout',exact:true}).click();
 await page.getByRole('button',{name:'Sign in',exact:true}).waitFor();
 assert.equal((await context.cookies()).filter(c=>c.name==='attos_session').length,0);
 await page.getByLabel('Username',{exact:true}).last().fill(user.username);
 await page.getByRole('button',{name:'Use passkey',exact:true}).click();
 await page.getByText('0/200',{exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/auth/passkey/options'),1);assert.equal(counts.get('/api/v1/auth/passkey/verify'),1);
 assert.equal((await context.cookies()).find(c=>c.name==='attos_session').value,'synthetic-a');
 await page.getByRole('button',{name:'Logout',exact:true}).click();
 await page.getByRole('button',{name:'Sign in',exact:true}).waitFor();
 assert.equal((await context.cookies()).filter(c=>c.name==='attos_session').length,0);
 await page.getByLabel('Username',{exact:true}).first().fill(other.username);
 await page.getByLabel('Password',{exact:true}).fill('synthetic-password-456');
 await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await page.getByText(other.username,{exact:true}).waitFor();
 await page.getByText('77/200',{exact:true}).waitFor();
 assert.equal((await context.cookies()).find(c=>c.name==='attos_session').value,'synthetic-b');
 assert.equal(await page.getByRole('button',{name:'Admin',exact:true}).count(),0);
 await page.getByRole('button',{name:'Logout',exact:true}).click();
 await page.getByRole('button',{name:'Sign in',exact:true}).waitFor();
 await page.setViewportSize({width:390,height:844});
 await context.clearCookies();
 const apiCountsBeforeDocs=[...counts].filter(([p])=>p.startsWith('/api/v1/'));
 await page.goto(origin+'/docs');
 await page.getByRole('heading',{name:JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')).info.title,exact:true}).waitFor();
 assert.equal(await page.getByRole('button',{name:'Sign in',exact:true}).count(),0);
 assert.equal(counts.get('/openapi.json'),1,'fresh public docs has exactly one OpenAPI GET owner');
 assert.deepEqual([...counts].filter(([p])=>p.startsWith('/api/v1/')),apiCountsBeforeDocs,'public docs sends no authenticated GETs or mutations');
 assert.deepEqual(errors,[],'release CSP and Wasm render without JS/browser errors');
 assert.deepEqual(failures,[]);
 console.log('Dioxus browser check passed: auth/onboarding/password-before-Cookie, admin/delete access, secret clearing, native WebAuthn registration/assertion, mutation acknowledgements, verification availability, reward schedule save/manual enqueue, ready/UID filter, single-flight Set-Cookie/owner consistency, failed logout/retry/reload, mobile docs, release CSP.');
}catch(error){console.error('Browser diagnostics:',JSON.stringify({errors,failures,requests:Object.fromEntries(counts),text:page?await page.locator('body').innerText().catch(()=>'<no page>'):''}));throw error;}finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
