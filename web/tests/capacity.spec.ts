import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
test('full intake shows operator capacity and permits exact retries without stopping the coordinator',async({page,context})=>{
 const auth={Authorization:'Bearer browser-test-operator-token-32-bytes'};const root=mkdtempSync(join(tmpdir(),'watchhouse-capacity-'));let pauseId='';let original:Record<string,number>|undefined;
 const get=async()=>{const r=await context.request.get('/api/v1/factory',{headers:auth});expect(r.ok()).toBe(true);return r.json();};
 const install=(limits:unknown)=>{const path=join(root,'limits.json');writeFileSync(path,JSON.stringify(limits));execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','factory-limits','--file',path,'--operator','operator']);};
 try {
  const initial=await get();original=initial.capacity.limits;
  const hold=await context.request.post('/api/v1/factory',{headers:auth,data:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Capacity browser fixture holds new fake execution'},expected_version:initial.version,action:{type:'pause',scope:'global',target:null,effect:'all',cancel_running:false}}});expect(hold.ok()).toBe(true);pauseId=(await hold.json()).pause_id;
  const before=await get();const max=before.capacity.queued_cases+1;install({...original,revision:before.capacity.limits.revision+1,max_queued_cases:max});
  const command={schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'One admitted capacity browser fixture',title:'Capacity fixture '+randomUUID(),description:'No live integration'};
  const first=await context.request.post('/api/v1/cases',{headers:auth,data:command});expect(first.status()).toBe(201);const id=(await first.json()).id;
  const rejected=await context.request.post('/api/v1/cases',{headers:auth,data:{...command,idempotency_key:randomUUID(),title:'Over-capacity fixture'}});expect(rejected.status()).toBe(429);expect(rejected.headers()['retry-after']).toBe('1');
  const retry=await context.request.post('/api/v1/cases',{headers:auth,data:command});expect(retry.status()).toBe(201);expect((await retry.json()).id).toBe(id);
  await page.goto('/');await page.getByLabel('Operator token').fill('browser-test-operator-token-32-bytes');await page.getByRole('button',{name:'Connect',exact:true}).click();await expect(page.locator('#factory-status')).toContainText(`Queue ${max}/${max}`);
  await page.locator('#title').fill('UI over-capacity fixture');await page.getByRole('button',{name:'Create case',exact:true}).click();await expect(page.locator('#error')).toContainText('Factory capacity reached for queued cases');
  expect((await get()).capacity.queued_cases).toBe(max);
 } finally {
  if(original){const s=await get();install({...original,revision:s.capacity.limits.revision+1});}
  if(pauseId){const s=await get();await context.request.post('/api/v1/factory',{headers:auth,data:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'End capacity browser fixture'},expected_version:s.version,action:{type:'resume',pause_id:pauseId}}});}
  rmSync(root,{recursive:true,force:true});
 }
});
