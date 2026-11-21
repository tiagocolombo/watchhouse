import {test,expect} from '@playwright/test';
import {randomUUID} from 'node:crypto';
test('operator pause survives reload while intake, observation and explicit resume remain available',async({page,context})=>{
 const auth={Authorization:'Bearer browser-test-operator-token-32-bytes'};const reason='Browser factory pause '+randomUUID();
 const get=async()=>{const r=await context.request.get('/api/v1/factory',{headers:auth});expect(r.ok()).toBe(true);return r.json();};
 try {
  await page.goto('/');await page.getByLabel('Operator token').fill('browser-test-operator-token-32-bytes');await page.getByRole('button',{name:'Connect',exact:true}).click();
  await page.getByText('Pause or stop execution',{exact:true}).click();await page.getByLabel('Control reason',{exact:true}).fill(reason);await page.getByRole('button',{name:'Apply execution pause',exact:true}).click();
  await expect(page.locator('#factory-pauses')).toContainText(reason);await expect(page.locator('#factory-pauses')).toContainText('Owner: operator');
  await page.locator('#title').fill('Queued under factory pause '+randomUUID());await page.getByRole('button',{name:'Create case',exact:true}).click();await expect(page.locator('#case-status')).toHaveText('queued');
  const id=await page.locator('#case-id').textContent();await page.reload();await expect(page.locator('#factory-pauses')).toContainText(reason);await expect(page.locator('#case-status')).toHaveText('queued');
  const before=await get();expect(before.pauses.some((p:{reason:string})=>p.reason===reason)).toBe(true);expect(before.active_while_paused).toContain('timers and escalation');
  await page.locator('#factory-pauses article').filter({hasText:reason}).getByRole('button',{name:'Resume this pause',exact:true}).click();await expect(page.locator('#factory-pauses')).not.toContainText(reason);await expect(page.locator('#case-status')).toHaveText('awaiting review');expect(await page.locator('#case-id').textContent()).toBe(id);
  const noAuth=await context.request.post('/api/v1/factory',{data:{}});expect(noAuth.status()).toBe(401);
 } finally {
  const current=await get();for(const pause of current.pauses.filter((p:{reason:string})=>p.reason===reason)){const s=await get();await context.request.post('/api/v1/factory',{headers:auth,data:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'End browser-owned pause fixture'},expected_version:s.version,action:{type:'resume',pause_id:pause.id}}});}
 }
});
