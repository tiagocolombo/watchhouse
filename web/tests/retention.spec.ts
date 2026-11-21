import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {randomUUID} from 'node:crypto';
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
