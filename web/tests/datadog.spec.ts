import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {randomUUID} from 'node:crypto';
test('Datadog coverage reports missing credentials without inventing healthy cases',async({page,context})=>{
  const auth={Authorization:'Bearer browser-test-operator-token-32-bytes'};
  execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','datadog','--file','../tests/fixtures/datadog/config.json','--operator','operator']);
  const r=await context.request.post('/api/v1/cases',{headers:auth,data:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Synthetic Datadog operating view',title:'Datadog coverage fixture',description:'No live credentials or source calls'}});expect(r.status()).toBe(201);
  await page.goto('/');await page.getByLabel('Operator token').fill('browser-test-operator-token-32-bytes');await page.getByRole('button',{name:'Connect',exact:true}).click();await page.getByRole('button',{name:/Datadog coverage fixture/}).click();
  await page.getByText('Datadog monitor coverage and capabilities',{exact:true}).click();await expect(page.locator('#datadog-health')).toContainText('no error-event equivalence');await expect(page.locator('#datadog-health')).toContainText('"api_credentials_present": false');
  await expect.poll(async()=>{const s=await context.request.get('/api/v1/sources/datadog',{headers:auth});return (await s.json())[0].reads.some((v:{state:string})=>v.state==='unavailable');}).toBe(true);
  const noAuth=await context.request.get('/api/v1/sources/datadog');expect(noAuth.status()).toBe(401);
  const cases=await(await context.request.get('/api/v1/cases',{headers:auth})).json();expect(cases.some((v:{title:string})=>/^Datadog monitor /.test(v.title))).toBe(false);
  await page.reload();await expect(page.locator('#datadog-health')).toContainText('current-state read unavailable');
});
