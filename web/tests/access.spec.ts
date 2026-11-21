import {test,expect} from '@playwright/test';
import {execFileSync,spawn} from 'node:child_process';
import {mkdtempSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import https from 'node:https';
import http from 'node:http';
test('configured identities and observer controls work through an actual local HTTPS proxy',async({browser})=>{
 test.setTimeout(60000);
 const dir=mkdtempSync(join(tmpdir(),'watchhouse-https-fixture-'));
 const db=join(dir,'state.db');const binary='../target/debug/watchhouse';
 const proxyToken='fixture-browser-proxy-32-byte-credential';const alice='fixture-browser-alice-32-byte-credential';const observer='fixture-browser-observer-32-byte-credential';const admin='fixture-browser-admin-32-byte-credential';const owner='fixture-browser-owner-32-byte-credential';
 let backend='';let child:ReturnType<typeof spawn>|undefined;
 let proxy:https.Server|undefined;
 const context=await browser.newContext({ignoreHTTPSErrors:true});
 try {
  execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',join(dir,'key.pem'),'-out',join(dir,'cert.pem'),'-sha256','-days','1','-subj','/CN=127.0.0.1','-addext','subjectAltName=IP:127.0.0.1'],{stdio:'ignore'});
  proxy=https.createServer({key:readFileSync(join(dir,'key.pem')),cert:readFileSync(join(dir,'cert.pem'))},(req,res)=>{
   if(!backend){res.writeHead(503);res.end();return;}
   // The proxy overwrites inbound forwarding assertions with its own values.
   const headers={...req.headers,host:backend,'x-forwarded-host':req.headers.host,'x-forwarded-proto':'https','x-watchhouse-proxy-token':proxyToken};
   const upstream=http.request({host:'127.0.0.1',port:Number(backend.split(':')[1]),path:req.url,method:req.method,headers},response=>{res.writeHead(response.statusCode??502,response.headers);response.pipe(res);});
   upstream.on('error',()=>{if(!res.headersSent)res.writeHead(502);res.end();});req.pipe(upstream);res.on('close',()=>upstream.destroy());
  });
  await new Promise<void>(resolve=>proxy!.listen(0,'127.0.0.1',resolve));
  const address=proxy.address();if(!address||typeof address==='string')throw new Error('missing proxy address');
  const origin=`https://127.0.0.1:${address.port}`;
  const policy={schema_version:1,revision:1,public_origin:origin,proxy_token_env:'WATCHHOUSE_REMOTE_PROXY',max_audit_rows:1000,paused:false,principals:[{id:'alice',role:'operator',token_env:'WATCHHOUSE_REMOTE_ALICE',disabled:false},{id:'bob',role:'observer',token_env:'WATCHHOUSE_REMOTE_BOB',disabled:false},{id:'admin',role:'admin',token_env:'WATCHHOUSE_REMOTE_ADMIN',disabled:false}]};
  const file=join(dir,'access.json');writeFileSync(file,JSON.stringify(policy));
  execFileSync(binary,['--db',db,'access-policy','--file',file,'--operator','operator']);
  child=spawn(binary,['--db',db,'serve','--remote','--port','0','--manual-workers'],{env:{...process.env,WATCHHOUSE_TOKEN:'ignored-in-remote-mode',WATCHHOUSE_REMOTE_PROXY:proxyToken,WATCHHOUSE_REMOTE_ALICE:alice,WATCHHOUSE_REMOTE_BOB:observer,WATCHHOUSE_REMOTE_ADMIN:admin,WATCHHOUSE_REMOTE_OWNER:owner},stdio:['ignore','ignore','pipe']});
  await new Promise<void>((resolve,reject)=>{let text='';const timer=setTimeout(()=>reject(new Error('remote fixture startup timed out')),15000);child!.once('exit',()=>{clearTimeout(timer);reject(new Error(`remote fixture exited: ${text}`));});child!.stderr!.on('data',data=>{text+=String(data);const match=text.match(/Watchhouse: http:\/\/(127\.0\.0\.1:\d+)/);if(match){backend=match[1];clearTimeout(timer);resolve();}});});
  const page=await context.newPage();await page.goto(origin);
  await page.getByLabel('Operator token').fill(alice);await page.getByRole('button',{name:'Connect',exact:true}).click();
  await expect(page.locator('#session-identity')).toHaveText('Remote access · alice · operator');
  await expect(page.locator('#factory-submit')).toBeDisabled();await expect(page.locator('#create')).toBeEnabled();
  await page.locator('#title').fill('Authenticated TLS fixture case');await page.locator('#description').fill('Synthetic data; no live service');await page.locator('#create').click();
  await expect(page.locator('#owner')).toHaveText('Owner: alice');
  const listed=await context.request.get(`${origin}/api/v1/cases`,{headers:{Authorization:`Bearer ${observer}`}});const cases=await listed.json();expect(cases).toHaveLength(1);expect(cases[0].owner).toBe('alice');
  const spoof=await context.request.post(`${origin}/api/v1/cases`,{headers:{Authorization:`Bearer ${alice}`},data:{schema_version:1,actor:'admin',idempotency_key:'browser-spoof',reason:'Fixture denial',title:'Forbidden',description:'Synthetic'}});expect(spoof.status()).toBe(403);
  const read=await context.newPage();await read.goto(origin);await read.getByLabel('Operator token').fill(observer);await read.getByRole('button',{name:'Connect',exact:true}).click();await expect(read.locator('#session-identity')).toHaveText('Remote access · bob · observer');await expect(read.locator('#create')).toBeDisabled();
  await read.getByRole('button',{name:/Authenticated TLS fixture case/}).click();await expect(read.locator('#owner')).toHaveText('Owner: alice');await expect(read.locator('#rename-title')).toBeDisabled();
  const foreign=await context.request.post(`${origin}/api/v1/cases`,{headers:{Authorization:`Bearer ${admin}`},data:{schema_version:1,actor:'admin',idempotency_key:'browser-foreign',reason:'Fixture scope denial',title:'Unrelated private fixture',description:'Only global roles may inspect this case'}});expect(foreign.status()).toBe(201);
  const scoped={...policy,revision:2,principals:[...policy.principals,{id:'service-owner',role:'service_owner',token_env:'WATCHHOUSE_REMOTE_OWNER',disabled:false,case_ids:[cases[0].id]}]};writeFileSync(file,JSON.stringify(scoped));execFileSync(binary,['--db',db,'access-policy','--file',file,'--operator','operator']);
  const owned=await context.newPage();await owned.goto(origin);await owned.getByLabel('Operator token').fill(owner);await owned.getByRole('button',{name:'Connect',exact:true}).click();await expect(owned.locator('#session-identity')).toHaveText('Remote access · service-owner · service_owner');await expect(owned.locator('#cases button')).toHaveCount(1);await expect(owned.locator('#cases')).not.toContainText('Unrelated private fixture');await expect(owned.locator('#create')).toBeDisabled();await expect(owned.getByRole('heading',{name:'Factory operation',exact:true})).not.toBeVisible();await owned.getByRole('button',{name:/Authenticated TLS fixture case/}).click();await expect(owned.locator('#rename-title')).toBeEnabled();await owned.getByText('Update title',{exact:true}).click();await owned.locator('#rename-title').fill('Service-owner authorized rename');await owned.locator('#rename-reason').fill('Granted case browser fixture');await owned.getByRole('button',{name:'Save with version check',exact:true}).click();await expect(owned.locator('#case-title')).toHaveText('Service-owner authorized rename');
  const hidden=await context.request.get(`${origin}/api/v1/cases/${(await foreign.json()).id}`,{headers:{Authorization:`Bearer ${owner}`}});expect(hidden.status()).toBe(403);
  scoped.revision=3;scoped.principals[0].disabled=true;writeFileSync(file,JSON.stringify(scoped));execFileSync(binary,['--db',db,'access-policy','--file',file,'--operator','operator']);
  const revoked=await context.request.get(`${origin}/api/v1/cases`,{headers:{Authorization:`Bearer ${alice}`}});expect(revoked.status()).toBe(401);
 } finally {
  await context.close();if(child){child.kill('SIGTERM');await new Promise<void>(resolve=>{if(child!.exitCode!==null)resolve();else child!.once('exit',()=>resolve());});}
  if(proxy){proxy.closeAllConnections();await new Promise<void>(resolve=>proxy!.close(()=>resolve()));}rmSync(dir,{recursive:true,force:true});
 }
});
