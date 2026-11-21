import {test,expect} from '@playwright/test';
import {execFileSync,spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
test('removed evidence exposes an inert tombstone and owned reassessment without restoring proof',async({page,context})=>{
 const key=randomUUID();const token='browser-test-operator-token-32-bytes';const headers={Authorization:`Bearer ${token}`};
 const created=await context.request.post('/api/v1/cases',{headers,data:{schema_version:1,idempotency_key:key,actor:'operator',reason:'Synthetic retention fixture',title:'Evidence removal fixture',description:'No real private data'}});expect(created.status()).toBe(201);const caseId=(await created.json()).id;
 await expect.poll(async()=>(await (await context.request.get(`/api/v1/cases/${caseId}`,{headers})).json()).case.status).toBe('awaiting_review');
 const snapshot=await (await context.request.get(`/api/v1/cases/${caseId}`,{headers})).json();
 const recorded=await context.request.post(`/api/v1/cases/${caseId}/evidence`,{headers,data:{schema_version:1,idempotency_key:`${key}-proof`,actor:'operator',reason:'Synthetic retained observation',expected_version:snapshot.case.version,scope:'Evidence-store fixture',statement:'Invented removable fixture body',collection_method:'Synthetic',observed_at:null,uncertainty:'No live service'}});expect(recorded.status()).toBe(201);const proof=(await recorded.json()).evidence.digest;
 const reason='Fixture logical removal <img src=x onerror=alert(1)>; retained case history is a separate copy';
 execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','evidence-redact','--digest',proof,'--operator','operator','--reason',reason]);
 const body=await (await context.request.get(`/api/v1/evidence/${proof}`,{headers})).json();expect(body.redacted).toBe(true);expect(JSON.stringify(body)).not.toContain('Invented removable fixture body');
 await page.goto(`/#${caseId}`);await page.getByLabel('Operator token').fill(token);await page.getByRole('button',{name:'Connect',exact:true}).click();await expect(page.locator('#case-redactions')).toContainText(reason);await expect(page.locator('#case-redactions')).toContainText('Collect replacement evidence');await expect(page.locator('#case-redactions img')).toHaveCount(0);await expect(page.locator('#team-escalations')).toContainText('Evidence removed');await expect(page.locator('#factory-pauses')).toContainText(caseId);await expect(page.locator('#factory-pauses')).toContainText('Evidence redacted');
 await page.reload();await expect(page.locator('#case-redactions')).toContainText(proof);await expect(page.locator('#case-redactions')).toContainText('Historical records and earlier exports');
});

test('configured expiry runs in a separate coordinator without a browser or executor',async({context})=>{
 const dir=mkdtempSync(join(tmpdir(),'watchhouse-retention-daemon-'));const db=join(dir,'state.db');const binary='../target/debug/watchhouse';const token='fixture-retention-daemon-32-byte-credential';
 const child=spawn(binary,['--db',db,'serve','--manual-workers','--port','0'],{env:{...process.env,WATCHHOUSE_TOKEN:token},stdio:['ignore','ignore','pipe']});
 try {
  const origin=await new Promise<string>((resolve,reject)=>{let text='';const timer=setTimeout(()=>reject(new Error('retention child startup timed out')),15000);child.once('exit',()=>{clearTimeout(timer);reject(new Error(`retention child exited: ${text}`));});child.stderr.on('data',data=>{text+=String(data);const match=text.match(/Watchhouse: (http:\/\/127\.0\.0\.1:\d+)/);if(match){clearTimeout(timer);resolve(match[1]);}});});
  const headers={Authorization:`Bearer ${token}`};
  const created=await context.request.post(`${origin}/api/v1/cases`,{headers,data:{schema_version:1,idempotency_key:'separate-retention-case',actor:'operator',reason:'Synthetic daemon schedule',title:'Isolated retention fixture',description:'No executor, model or live source'}});expect(created.status()).toBe(201);const c=await created.json();
  const response=await context.request.post(`${origin}/api/v1/cases/${c.id}/evidence`,{headers,data:{schema_version:1,idempotency_key:'separate-retention-proof',actor:'operator',reason:'Synthetic expiring body',expected_version:c.version,scope:'Separate temporary database',statement:'Invented body for scheduled expiry',collection_method:'Fixture only',observed_at:null,uncertainty:'No actual incident'}});expect(response.status()).toBe(201);const proof=(await response.json()).evidence.digest;
  const policy={schema_version:1,revision:1,paused:false,interval_seconds:1,retain_seconds:{normalized_source:null,execution_evidence:null,reported_evidence:1,supervisor_checkpoint:null,simulation:null,other:null}};const file=join(dir,'retention.json');writeFileSync(file,JSON.stringify(policy));execFileSync(binary,['--db',db,'retention-policy','--file',file,'--operator','operator']);
  // No page has opened this coordinator; the deterministic brain owns expiry.
  await expect.poll(async()=>(await (await context.request.get(`${origin}/api/v1/evidence/${proof}`,{headers})).json()).redacted,{timeout:15000}).toBe(true);
  const status=await (await context.request.get(`${origin}/api/v1/retention`,{headers})).json();expect(status.removed_bodies).toBe(1);expect(status.owner).toBe('operator');expect(status.restored_authority_held).toBe(false);
  const page=await context.newPage();await page.goto(`${origin}/#${c.id}`);await page.getByLabel('Operator token').fill(token);await page.getByRole('button',{name:'Connect',exact:true}).click();await expect(page.locator('#case-redactions')).toContainText('Scheduled evidence-store expiry');await expect(page.locator('#factory-status')).toContainText('Evidence-body expiry configured');await expect(page.locator('#team-escalations')).toContainText('Evidence removed');await expect(page.locator('#case-status')).toHaveText('queued');await page.close();
 } finally {child.kill('SIGTERM');await new Promise<void>(resolve=>{if(child.exitCode!==null)resolve();else child.once('exit',()=>resolve());});rmSync(dir,{recursive:true,force:true});}
});
