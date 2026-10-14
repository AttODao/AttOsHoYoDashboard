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
const user={id:uid(1),username:'synthetic-admin',is_admin:true,active:true,must_change_password:false};
const other={...user,id:uid(9),username:'synthetic-other',is_admin:false};
let verified=false,passkeys=[],failLogout=false,allowLogin,allowSettings;
const loginGate=new Promise(resolve=>{allowLogin=resolve;});
const settingsGate=new Promise(resolve=>{allowSettings=resolve;});
const counts=new Map();const failures=[];
const accounts=[{id:uid(3),visible:false,detected:true,game_uid:'600000003'},{id:uid(4),visible:true,detected:false,game_uid:'600000004'},{id:uid(5),visible:true,detected:true,game_uid:'600000005'}].map(a=>({...a,hoyolab_account_id:uid(2),game_kind:'genshin',region:'os_usa',last_detected_at:new Date().toISOString()}));
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
   if(!actor)return json(res,{error:'unauthorized'},401);
   if(p==='/api/v1/me')return json(res,{...actor,onboarding:{password_change_required:false,cookie_setup_complete:verified,cookie_status:verified?'verified':'pending_verification',verification_available:false},capabilities:{reward_posting_available:false}});
   if(p==='/api/v1/me/hoyolab')return json(res,{id:uid(2),hoyolab_uid:verified?'123':null,status:verified?'verified':'pending_verification',setup_complete:verified,updated_at:null,verification_available:false});
   if(p==='/api/v1/me/hoyolab/cookie'){assert.ok(body.cookie.startsWith('ltuid_v2=123;'));return json(res,{status:'pending_verification',setup_complete:false},202);}
   if(p==='/api/v1/me/passkeys/options')return json(res,{challenge_id:uid(10),options:{publicKey:{challenge:Buffer.from('synthetic-browser-challenge-32-bytes').toString('base64url'),rp:{id:'localhost',name:'Synthetic UI check'},user:{id:Buffer.from('synthetic-user').toString('base64url'),name:user.username,displayName:user.username},pubKeyCredParams:[{type:'public-key',alg:-7}],timeout:60000,authenticatorSelection:{userVerification:'required',residentKey:'preferred'},attestation:'none'}}});
   if(p==='/api/v1/me/passkeys/verify'){assert.ok(body.response.response.clientDataJSON);assert.ok(body.response.response.attestationObject);assert.equal(body.response.type,'public-key');passkeys=[{id:uid(11),label:'Synthetic browser key',created_at:new Date().toISOString()}];return json(res,{});}
   if(p==='/api/v1/me/passkeys')return json(res,passkeys);
   if(p==='/api/v1/server-settings'){await settingsGate;return json(res,{timezone_offset:'+09:00',session_ttl_days:7});}
   if(p==='/api/v1/admin/users')return json(res,[user,{...user,id:uid(9),username:'synthetic-other',is_admin:false,active:false}]);
   if(p==='/api/v1/game-accounts')return json(res,accounts);
   if(p===`/api/v1/game-accounts/${uid(5)}/status`)return json(res,{...accounts[2],data:{current_resin:actor===user?0:77,max_resin:200,finished_task_num:0,total_task_num:4,expeditions:[]},fetched_at:new Date().toISOString(),error_code:null,error_at:null,stale:false});
   if(p==='/api/v1/reward-settings')return json(res,['genshin','starrail'].map((game_kind,i)=>({id:uid(20+i),game_kind,enabled:false,local_time:'06:00',auth_status:'ready',next_run_at:null,last_run:null,is_sign:!!i,last_checked_at:new Date().toISOString(),error_code:null,error_at:null})));
   if(p==='/api/v1/reward-runs')return json(res,[]);
   throw new Error('Unexpected implemented-API request: '+req.method+' '+p);
  }
  if(p==='/openapi.json')return json(res,JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')));
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
 const metadataRequest=page.waitForRequest(origin+'/api/v1/server-settings');allowLogin();await metadataRequest;
 await page.getByText('Setup status',{exact:true}).waitFor();
 assert.ok(await page.getByRole('button',{name:'Refresh setup status',exact:true}).isDisabled(),'busy survives the bootstrap state reset');
 await page.getByRole('button',{name:'Refresh setup status',exact:true}).dispatchEvent('click');
 allowSettings();
 await page.getByRole('button',{name:'Re-enable',exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/auth/login'),1,'competing password/passkey login was rejected before enqueueing');
 assert.equal(counts.get('/api/v1/auth/passkey/options')||0,0);
 assert.equal(counts.get('/api/v1/me'),2,'bootstrap refresh cannot interrupt login bootstrap');
 assert.equal((await context.cookies()).find(c=>c.name==='attos_session').value,'synthetic-a');
 await page.getByRole('button',{name:'Delete my account',exact:true}).waitFor();
 assert.equal(counts.get('/api/v1/game-accounts')||0,0);
 assert.equal(counts.get('/api/v1/reward-settings')||0,0);
 await page.getByLabel('Paste one HoYoLAB Cookie').fill('ltuid_v2=123; ltoken_v2=synthetic-browser-token');
 await page.getByRole('button',{name:'Save Cookie',exact:true}).click();
 await page.getByText('Setup status',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('Paste one HoYoLAB Cookie').inputValue(),'');
 const cdp=await context.newCDPSession(page);await cdp.send('WebAuthn.enable');
 await cdp.send('WebAuthn.addVirtualAuthenticator',{options:{protocol:'ctap2',transport:'internal',hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}});
 await page.getByLabel('New passkey label').fill('Synthetic browser key');
 await page.getByRole('button',{name:'Register passkey',exact:true}).click();
 await page.getByText(/Synthetic browser key — created/).waitFor();
 // A fixture-only change exercises the ready branch; no production proof gate is changed.
 verified=true;
 await page.getByRole('button',{name:'Refresh setup status',exact:true}).click();
 await page.getByRole('heading',{name:'Dashboard',exact:true}).waitFor();
 await page.getByText('0/200',{exact:true}).waitFor();
 assert.equal(await page.locator('#selected-account option').count(),1);
 assert.equal(counts.get(`/api/v1/game-accounts/${uid(3)}/status`)||0,0);
 assert.equal(counts.get(`/api/v1/game-accounts/${uid(4)}/status`)||0,0);
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
 await page.goto(origin+'/docs');
 await page.getByRole('heading',{name:JSON.parse(fs.readFileSync(path.join(root,'docs/dashboard-api.json'),'utf8')).info.title,exact:true}).waitFor();
 assert.equal(await page.getByRole('button',{name:'Sign in',exact:true}).count(),0);
 assert.deepEqual(errors,[],'release CSP and Wasm render without JS/browser errors');
 assert.deepEqual(failures,[]);
 console.log('Dioxus browser check passed: auth/onboarding, admin/delete access, secret clearing, synthetic native WebAuthn, ready/UID filter, single-flight Set-Cookie/owner consistency, failed logout/retry/reload, mobile docs, release CSP.');
}catch(error){console.error('Browser diagnostics:',JSON.stringify({errors,failures,requests:Object.fromEntries(counts),text:page?await page.locator('body').innerText().catch(()=>'<no page>'):''}));throw error;}finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
