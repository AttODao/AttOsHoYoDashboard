// Design-artifact checks only; not backend or production-Dioxus verification.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url));
const tokens=JSON.parse(fs.readFileSync(path.join(root,'tokens.json'),'utf8'));
const scripts=fs.readdirSync(path.join(root,'penpot')).filter(n=>n.endsWith('.js'));
const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
for(const name of scripts)new AsyncFunction(fs.readFileSync(path.join(root,'penpot',name),'utf8'));
const setup=fs.readFileSync(path.join(root,'penpot/setup.js'),'utf8');
const colorBlock=setup.match(/const colors = (\{[\s\S]*?\n\});/)[1];
const palette=Function('return ('+colorBlock+')')();
for(const [name,value] of Object.entries(palette))assert.equal(tokens.color[name].$value,value,'palette drift: '+name);
function luminance(hex){const c=hex.slice(1).match(/../g).map(n=>parseInt(n,16)/255).map(n=>n<=0.04045?n/12.92:((n+.055)/1.055)**2.4);return .2126*c[0]+.7152*c[1]+.0722*c[2];}
function contrast(fg,bg){const [a,b]=[luminance(palette[fg]),luminance(palette[bg])].sort((a,b)=>b-a);return(a+.05)/(b+.05);}
const textPairs=[['text','canvas'],['text','surface'],['text','rail'],['muted','elevated'],['onPrimary','primary'],['genshin','genshinSurface'],['starrail','starrailSurface'],['warning','warningSurface'],['error','errorSurface'],['success','successSurface']];
for(const [fg,bg] of textPairs)assert.ok(contrast(fg,bg)>=4.5,fg+'/'+bg+' text contrast');
for(const bg of ['canvas','surface','elevated','genshinSurface'])assert.ok(contrast('line',bg)>=3,'control boundary on '+bg);
assert.ok(tokens.layout.controlHeight>=44);
assert.equal(tokens.layout.mobile,390);
assert.ok(scripts.includes('bind.js'),'reconnect recovery script');
console.log(`${scripts.length} scripts parse; ${Object.keys(palette).length} palette values match; ${textPairs.length} text pairs >=4.5:1; control boundaries >=3:1.`);
console.log('Native geometry/interaction/font propagation still require Penpot inspection.');
