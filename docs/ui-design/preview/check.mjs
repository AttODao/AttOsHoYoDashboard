// Synthetic UI checks only; no production API or credential operations.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {captureScene} from './scene.mjs';
const {chromium}=createRequire(import.meta.url)(process.env.ATTOS_TEST_PLAYWRIGHT_MODULE||'playwright');
const root=path.dirname(fileURLToPath(import.meta.url)),out=process.env.ATTOS_DESIGN_CAPTURE_DIR||'/tmp/agd-r3';fs.mkdirSync(out,{recursive:true});
const source=fs.readFileSync(path.join(root,'preview.js'),'utf8');
assert.doesNotMatch(source,/\b(fetch|XMLHttpRequest|sessionStorage|PublicKeyCredential)\s*[.(]/);
const sourceHash=createHash('sha256').update(['index.html','style.css','preview.js','i18n.js','scene.mjs'].map(n=>fs.readFileSync(path.join(root,n),'utf8')).join('\0')).digest('hex');
const server=http.createServer((req,res)=>{const name=new URL(req.url,'http://localhost').pathname,file={'/':'index.html','/index.html':'index.html','/style.css':'style.css','/preview.js':'preview.js','/i18n.js':'i18n.js'}[name];if(!file){res.writeHead(404);res.end();return;}res.setHeader('Content-Type',file.endsWith('.css')?'text/css':file.endsWith('.js')?'text/javascript':'text/html; charset=utf-8');res.end(fs.readFileSync(path.join(root,file)));});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const origin='http://127.0.0.1:'+server.address().port;
let browser;const errors=[],unsafe=[],manifest=[];
try{
 browser=await chromium.launch({headless:true});const context=await browser.newContext({reducedMotion:'reduce'});
 await context.route('**/*',route=>{const r=route.request(),url=new URL(r.url());if((url.origin!==origin&&!['fonts.googleapis.com','fonts.gstatic.com'].includes(url.hostname))||['fetch','xhr'].includes(r.resourceType())){unsafe.push(url.hostname+url.pathname);return route.abort();}return route.continue();});
 const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));
 await page.setViewportSize({width:1440,height:1000});await page.goto(origin+'/#login');
 assert.equal(await page.locator('#username').count(),1);assert.equal(await page.locator('.auth-copy').count(),0);assert.equal(await page.locator('.auth-panel p').count(),0);
 const login=await page.locator('.auth-panel').boundingBox(),password=await page.locator('#password-field input').boundingBox(),submit=await page.locator('[data-action=sign-in]').boundingBox();
 assert.ok(Math.abs(login.x+login.width/2-720)<2,'centered login');assert.ok(submit.y-password.y-password.height>=24,'password-to-submit gap');
 const eye=page.getByRole('button',{name:'パスワードを表示',exact:true});await eye.click();assert.equal(await page.locator('#password-field input').getAttribute('type'),'text');assert.equal(await page.getByRole('button',{name:'パスワードを隠す'}).getAttribute('aria-pressed'),'true');await page.getByRole('button',{name:'パスワードを隠す'}).click();assert.equal(await page.locator('#password-field input').getAttribute('type'),'password');
 await page.getByRole('radio',{name:'パスキー',exact:true}).check();assert.equal(await page.locator('#password-field').isVisible(),false);
 await page.getByRole('button',{name:'パスキーでログイン'}).click();
 assert.equal(await page.locator('.game-panel:visible').count(),2,'desktop games simultaneous');assert.equal(await page.locator('.game-tabs').isVisible(),false);
 for(const key of ['genshin','starrail'])assert.equal(await page.locator('.game-panel.'+key).getByRole('switch').count(),1,'switch in owning card');
 assert.notEqual(await page.locator('.genshin').evaluate(n=>getComputedStyle(n).backgroundColor),await page.locator('.starrail').evaluate(n=>getComputedStyle(n).backgroundColor));
 assert.equal(await page.getByLabel('メニューを開く').innerText(),'');assert.equal(await page.locator('main input[type=time]').count(),0);
 const before=await page.locator('.dashboard-grid').boundingBox();await page.getByLabel('メニューを開く').click();assert.equal(await page.locator('[data-action=admin]').count(),0);
 const drawer=await page.getByRole('dialog').boundingBox();assert.equal(drawer.width,304);assert.equal(drawer.x+drawer.width,1440);assert.equal(drawer.y,0);assert.equal(drawer.height,1000);assert.deepEqual(await page.locator('.dashboard-grid').boundingBox(),before);
 await page.getByRole('button',{name:'English',exact:true}).click();assert.equal(await page.locator('html').getAttribute('lang'),'en');assert.equal(await page.getByRole('dialog',{name:'Account menu'}).count(),1);await page.getByRole('switch',{name:'Dark mode'}).click();assert.equal(await page.locator('html').getAttribute('data-theme'),'light');assert.equal(await page.getByRole('switch',{name:'Dark mode'}).getAttribute('aria-checked'),'false');
 await page.keyboard.press('Escape');assert.equal(await page.getByLabel('Open menu').evaluate(n=>n===document.activeElement),true);await page.reload();assert.equal(await page.locator('html').getAttribute('lang'),'en');assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
 await page.getByLabel('Open menu').click();await page.mouse.click(30,500);assert.equal(await page.getByRole('dialog').count(),0);await page.getByLabel('Open menu').click();await page.getByRole('button',{name:'日本語',exact:true}).click();await page.getByRole('switch',{name:'ダークモード'}).click();await page.locator('[data-action=settings]').click();
 assert.equal(await page.getByRole('tabpanel').count(),1,'settings switches one panel, not scroll buttons');
 await page.getByRole('tab',{name:'ログインボーナス',exact:true}).click();assert.equal(await page.getByRole('tab',{name:'ログインボーナス'}).getAttribute('aria-selected'),'true');
 assert.equal(await page.locator('[data-visible]').count(),0,'inactive panel not present');assert.equal(await page.locator('.timezone').count(),2);
 await page.getByLabel('原神の時',{exact:true}).selectOption({label:'09'});await page.getByLabel('原神の分',{exact:true}).selectOption({label:'37'});await page.locator('[data-action=save-genshin]').click();
 await page.getByRole('tab',{name:'セキュリティ',exact:true}).click();await page.getByRole('tab',{name:'ログインボーナス',exact:true}).click();assert.equal(await page.getByLabel('原神の分',{exact:true}).inputValue(),'37');
 await page.keyboard.press('ArrowRight');assert.equal(await page.getByRole('tab',{name:'HoYoLAB連携',exact:true}).getAttribute('aria-selected'),'true');assert.equal(await page.getByRole('tab',{name:'HoYoLAB連携',exact:true}).evaluate(n=>n===document.activeElement),true);
 await page.goto(origin+'/?fixture=provisional#setup');assert.equal(await page.getByLabel('現在のパスワード',{exact:true}).count(),0);assert.equal(await page.locator('.menu').count(),0);assert.equal(await page.locator('input[type=password]').count(),2);
 await page.evaluate(()=>location.hash='settings');await page.waitForFunction(()=>location.hash==='#setup');await page.locator('[data-action=setup-password]').click();
 assert.equal(await page.locator('[data-action=setup-cookie]').count(),1);assert.equal(await page.locator('[data-action=setup-verify]').isDisabled(),true);await page.locator('[data-action=setup-cookie]').click();assert.match(await page.locator('main').innerText(),/本人確認待ち/);assert.equal(await page.locator('.menu').count(),0);
 await page.locator('[data-action=setup-verify]').click();assert.equal(await page.locator('.game-panel .empty').count(),2,'new UID initially hidden');
 await page.goto(origin+'/?fixture=recovery#setup');assert.equal(await page.locator('input[type=password]').count(),2);assert.equal(await page.getByLabel('現在のパスワード',{exact:true}).count(),0);assert.equal(await page.locator('[data-action=setup-cookie],[data-action=setup-verify]').count(),0,'reissue has no Cookie registration');
 await page.locator('[data-action=setup-password]').click();assert.match(await page.title(),/ダッシュボード/);assert.equal(await page.locator('.game-panel .empty').count(),0,'verified recovery fixture retains linkage');
 await page.goto(origin+'/?fixture=admin#admin');assert.equal(await page.locator('input[type=password]').count(),0,'admin does not specify generated password');
 await page.locator('[data-action=admin-create]').click();assert.equal(await page.getByRole('dialog').locator('output').count(),1);assert.match(await page.locator('output').innerText(),/^DEMO-ONLY-/);await page.locator('[data-action=cancel-delete]').click();
 await page.locator('[data-action=admin-reset-1]').click();assert.equal(await page.getByRole('dialog').locator('input').count(),0);await page.locator('[data-action=accept-admin-reset-demo_inactive]').click();assert.equal(await page.getByRole('dialog').locator('output').count(),1);await page.locator('[data-action=cancel-delete]').click();
 await page.getByLabel('メニューを開く').click();assert.equal(await page.locator('[data-action=admin]').count(),1);await page.keyboard.press('Escape');
 await page.goto(origin+'/?state=unknown&tab=bonus#settings');assert.equal(await page.locator('[data-action=run-genshin]').isDisabled(),true);assert.match(await page.locator('main').innerText(),/再送せず/);
 await page.goto(origin+'/?state=queued&tab=bonus#settings');assert.match(await page.locator('main').innerText(),/受け取り完了ではありません/);
 await page.setViewportSize({width:390,height:1000});await page.goto(origin+'/#dashboard');assert.equal(await page.locator('.game-panel:visible').count(),1);assert.equal(await page.getByRole('tablist',{name:'ゲーム'}).isVisible(),true);
 await page.getByRole('switch',{name:'原神の自動受け取り'}).check();await page.getByRole('tab',{name:'崩壊：スターレイル',exact:true}).click();assert.equal(await page.locator('.starrail').isVisible(),true);await page.keyboard.press('Home');assert.equal(await page.getByRole('switch',{name:'原神の自動受け取り'}).isChecked(),true);
 async function capture(name,width){
  await page.evaluate(()=>document.fonts.ready);assert.ok(await page.evaluate(()=>['Noto Sans JP','Orbitron'].every(name=>Array.from(document.fonts).some(f=>f.family.replace(/["']/g,'')===name&&f.status==='loaded'))),'real fonts loaded');
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),name+' overflow');
  const small=await page.locator('button,summary,select,.switch,.check').evaluateAll(nodes=>nodes.filter(n=>n.getClientRects().length&&!n.disabled).map(n=>({label:n.textContent.trim().slice(0,30),w:n.getBoundingClientRect().width,h:n.getBoundingClientRect().height})).filter(n=>n.w<44||n.h<44));assert.deepEqual(small,[],name+' targets');
  const scene=await page.evaluate(captureScene,name+'-'+width),nodes=[];const walk=n=>{nodes.push(n);(n.children||[]).forEach(walk);};scene.children.forEach(walk);assert.equal(scene.source,'agd-r3-local-synthetic');assert.ok(nodes.every(n=>[n.x,n.y,n.width,n.height].every(Number.isFinite)&&n.width>0&&n.height>0));
  if(!name.startsWith('menu'))assert.equal(nodes.some(n=>/^nav \/ (アカウントメニュー|Account menu)/.test(n.name)),false);
  assert.ok(nodes.filter(n=>n.inputType==='password').every(n=>n.children[0].text===' '));assert.doesNotMatch(JSON.stringify(scene),/自己削除|AttOs|デザイン確認用/);
  scene.sourceHash=sourceHash;fs.writeFileSync(path.join(out,scene.key+'.json'),JSON.stringify(scene));manifest.push(scene.key);await page.screenshot({path:path.join(out,scene.key+'.png'),fullPage:!await page.getByRole('dialog').count()});
 }
 const screens=[['login','/#login'],['dashboard','/#dashboard'],['setup','/?fixture=provisional#setup'],['recovery','/?fixture=recovery#setup'],['settings','/#settings'],['settings-bonus','/?tab=bonus#settings'],['settings-link','/?tab=link#settings'],['settings-security','/?tab=security#settings'],['settings-delete','/?tab=delete#settings'],['admin','/?fixture=admin#admin'],['unknown','/?state=unknown&tab=bonus#settings'],['queued','/?state=queued&tab=bonus#settings']];
 for(const width of [1440,390]){
  await page.setViewportSize({width,height:1000});for(const [name,url]of screens){await page.goto(origin+url);await capture(name,width);}
  await page.goto(origin+'/?fixture=provisional#setup');await page.locator('[data-action=setup-password]').click();await capture('setup-cookie',width);await page.locator('[data-action=setup-cookie]').click();await capture('setup-pending',width);
  await page.goto(origin+'/#dashboard');if(width===390){await page.getByRole('tab',{name:'崩壊：スターレイル',exact:true}).click();await capture('dashboard-starrail',width);}await page.getByLabel('メニューを開く').click();await capture('menu',width);await page.keyboard.press('Escape');
  await page.goto(origin+'/?fixture=admin#admin');await page.getByLabel('メニューを開く').click();await capture('menu-admin',width);await page.keyboard.press('Escape');
  await page.locator('[data-action=admin-create]').click();await capture('admin-created',width);await page.locator('[data-action=cancel-delete]').click();
  await page.locator('[data-action=admin-reset-1]').click();await capture('admin-recovery',width);await page.locator('[data-action=accept-admin-reset-demo_inactive]').click();await capture('admin-reissued',width);await page.locator('[data-action=cancel-delete]').click();
  await page.locator('[data-action=admin-delete-1]').click();await capture('admin-deletion',width);await page.locator('[data-action=accept-admin-delete]').click();await capture('admin-deleted',width);
 }
 for(const width of [1440,390]){
  await page.setViewportSize({width,height:1000});
  for(const [suffix,lang,theme]of [['en-dark','en','dark'],['ja-light','ja','light'],['en-light','en','light']]){
   await page.goto(origin+'/?lang='+lang+'&theme='+theme+'#dashboard');await capture('dashboard-'+suffix,width);await page.getByLabel(lang==='en'?'Open menu':'メニューを開く').click();await capture('menu-'+suffix,width);await page.keyboard.press('Escape');
  }
  for(const [name,url]of [['login-en-light','?lang=en&theme=light#login'],['settings-security-en-light','?lang=en&theme=light&tab=security#settings']]){await page.goto(origin+'/'+url);await capture(name,width);}
 }
 // All translated responsibilities, not just the representative Native frames.
 for(const theme of ['dark','light'])for(const [name,url]of screens){
  // Use URLSearchParams rather than depend on the fixture's query ordering.
  const u=new URL(origin+url);u.searchParams.set('lang','en');u.searchParams.set('theme',theme);await page.goto(u.href);
  const japanese=await page.locator('main').innerText();assert.doesNotMatch(japanese,/[ぁ-んァ-ヶ一-龯]/,name+' untranslated UI');assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),name+' English overflow');
 }
 await page.goto(origin+'/?lang=ja&theme=dark&privacy=check#login');await page.evaluate(()=>{Object.defineProperty(HTMLInputElement.prototype,'value',{get(){throw Error('Credential value getter forbidden');}});const get=HTMLInputElement.prototype.getAttribute;HTMLInputElement.prototype.getAttribute=function(k){if(k==='value'&&this.type!=='radio')throw Error('Credential value attr forbidden');return get.call(this,k);};});await page.evaluate(()=>document.fonts.ready);await page.getByRole('button',{name:'パスワードを表示',exact:true}).click();const privateScene=await page.evaluate(captureScene,'privacy-390');assert.doesNotMatch(JSON.stringify(privateScene),/DEMO-ONLY-/,'never inspect a shown credential');
 for(const width of [360,768,1280]){await page.setViewportSize({width,height:1000});await page.goto(origin+'/#dashboard');assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'fluid '+width);}
 await page.setViewportSize({width:1440,height:1000});await page.goto(origin+'/#dashboard');
 const colors=await page.locator('.game-panel').evaluateAll(nodes=>nodes.map(n=>({bg:getComputedStyle(n).backgroundColor,text:getComputedStyle(n).color,muted:getComputedStyle(n.querySelector('small')).color})));
 const luminance=c=>{const v=c.match(/[\d.]+/g).slice(0,3).map(Number).map(n=>n/255).map(n=>n<=.04045?n/12.92:((n+.055)/1.055)**2.4);return v[0]*.2126+v[1]*.7152+v[2]*.0722;};
 for(const c of colors)for(const key of ['text','muted']){const a=luminance(c.bg),b=luminance(c[key]);assert.ok((Math.max(a,b)+.05)/(Math.min(a,b)+.05)>=4.5,'colored card '+key+' contrast');}
 for(const url of ['/?fixture=provisional#setup','/?fixture=recovery#setup','/?tab=security#settings','/?tab=delete#settings']){
  await page.goto(origin+url);const inputs=page.locator('[data-password]');assert.equal(await inputs.count(),await page.locator('.password-eye').count());await page.locator('.password-eye').last().click();assert.equal(await inputs.last().getAttribute('type'),'text');await page.evaluate(()=>window.dispatchEvent(new Event('blur')));assert.equal(await inputs.last().getAttribute('type'),'password');
 }
 await page.goto(origin+'/?theme=light#dashboard');
 const light=await page.locator('.game-panel').evaluateAll(nodes=>nodes.map(n=>({bg:getComputedStyle(n).backgroundColor,text:getComputedStyle(n).color,muted:getComputedStyle(n.querySelector('small')).color})));
 for(const c of light)for(const key of ['text','muted']){const a=luminance(c.bg),b=luminance(c[key]);assert.ok((Math.max(a,b)+.05)/(Math.min(a,b)+.05)>=4.5,'light card '+key+' contrast');}
 await page.emulateMedia({reducedMotion:'no-preference'});await page.getByLabel('メニューを開く').click();const motion=await page.locator('#account-menu').evaluate(async d=>{const x=[];for(let i=0;i<25;i++){await new Promise(requestAnimationFrame);x.push(d.getBoundingClientRect().x);}return x;});assert.ok(motion[0]>motion.at(-1)+10,'right-to-left entrance');assert.ok(Math.abs(motion.at(-1)-(1440-304))<2);await page.keyboard.press('Escape');await page.emulateMedia({reducedMotion:'reduce'});
 const stored=await page.evaluate(()=>Object.fromEntries(Object.entries(localStorage)));assert.deepEqual(Object.keys(stored).sort(),['agd-preview-lang','agd-preview-theme']);assert.equal(stored['agd-preview-lang'],'ja');assert.equal(stored['agd-preview-theme'],'dark');
 assert.deepEqual(unsafe,[]);assert.deepEqual(errors,[]);fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify({sourceHash,keys:manifest}));
 console.log('PASS R3: password visibility, drawer dismissal/focus, persistent language/theme preferences, full English surfaces, centered/gapped login, desktop dual games, colored cards/own switches, icon menu/role gate, accessible switching tabs, hour/minute/timezone, initial versus recovery setup, generated-password mock, deletion labels, queued/unknown and credential-capture privacy.');console.log(manifest.length+' scenes + PNGs in '+out+'; synthetic only.');
}finally{if(browser)await browser.close();await new Promise(r=>server.close(r));}
