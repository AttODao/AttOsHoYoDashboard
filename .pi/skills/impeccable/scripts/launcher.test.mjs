// Synthetic local release; never download or execute an unverified remote fixture.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp, mkdir, copyFile, writeFile, symlink, rm, access} from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import {spawn, execFileSync} from 'node:child_process';

const root=await mkdtemp(path.join(os.tmpdir(),'ahd-impeccable-'));
const scripts=path.join(root,'.pi/skills/impeccable/scripts');
const bin=path.join(root,'bin');
const engine='#!/bin/sh\nprintf "impeccable-engine synthetic\\n"\n';
const digest=createHash('sha256').update(engine).digest('hex');
let badChecksum=false,requests=0;
const server=http.createServer((req,res)=>{
 requests++;
 res.end(req.url.endsWith('.sha256')?(badChecksum?'0'.repeat(64):digest)+'  impeccable-linux-x64\n':engine);
});
try {
 await mkdir(scripts,{recursive:true});await mkdir(bin);
 await copyFile(new URL('./impeccable',import.meta.url),path.join(scripts,'impeccable'));
 await writeFile(path.join(scripts,'VERSION'),'synthetic');
 // Exclude curl/wget so this tests the project's installed Node fallback.
 for(const name of ['node','uname','dirname','tr','mkdir','cut','sha256sum','rm','chmod','mv']) {
  const executable=execFileSync('/bin/sh',['-c','command -v "$1"','sh',name],{encoding:'utf8'}).trim();
  await symlink(executable,path.join(bin,name));
 }
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const env={PATH:bin,HOME:path.join(root,'home'),IMPECCABLE_DOWNLOAD_BASE:`http://127.0.0.1:${server.address().port}`};
 const run=()=>new Promise((resolve,reject)=>{
  const child=spawn('/bin/sh',[path.join(scripts,'impeccable'),'engine-probe'],{env});
  let output='';child.stdout.on('data',data=>output+=data);child.stderr.on('data',data=>output+=data);
  child.on('error',reject);child.on('close',code=>resolve({code,output}));
 });
 const first=await run();assert.equal(first.code,0,first.output);assert.match(first.output,/impeccable-engine synthetic/);
 const cache=path.join(root,'.impeccable/cache/bin/synthetic/impeccable');await access(cache);
 await assert.rejects(access(path.join(root,'home/.impeccable')),'user cache is untouched');
 assert.equal(requests,2);assert.equal((await run()).code,0);assert.equal(requests,2,'cached engine needs no download');
 await rm(cache);badChecksum=true;
 const rejected=await run();assert.notEqual(rejected.code,0);assert.match(rejected.output,/checksum mismatch/);
 await assert.rejects(access(cache),'mismatched engine is not cached');
 console.log('Impeccable launcher: project cache, Node download, reuse and checksum rejection passed');
} finally {
 await new Promise(resolve=>server.close(resolve));await rm(root,{recursive:true,force:true});
}
