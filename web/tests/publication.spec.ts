import {test,expect} from '@playwright/test';
import {randomUUID} from 'node:crypto';
test('publication observations stay inert and visibly separate from merge and recovery',async({page,context})=>{
  const token='browser-test-operator-token-32-bytes';const auth={Authorization:`Bearer ${token}`};
  const response=await context.request.post('/api/v1/cases',{headers:auth,data:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Synthetic publication view fixture',title:'Publisher observation fixture',description:'Mocked view; no GitHub calls'}});expect(response.status()).toBe(201);const id=(await response.json()).id;
  // View-only injection exercises escaping and reload; domain/controller acceptance is in Rust.
  await page.route(`**/api/v1/cases/${id}`,async route=>{
    const original=await route.fetch();const snapshot=await original.json();
    snapshot.merges=[{id:'merge-view-fixture',state:'unknown',owner:'release-owner',native:false,deadline:'2026-11-17T12:00:00Z',reason:'<script>window.mergeInjected=true</script> Reconcile exact head/base; never replay'}];
    snapshot.publications=[{id:'pub-view-fixture',state:'quarantined',stage:'pr',owner:'operator',native:false,deadline:'2026-11-16T12:00:00Z',reason:'<img src=x onerror="window.publicationInjected=true"> Lost acknowledgment; inspect exact effect',branch:'watchhouse/repair-view-fixture',pr:null}];
    await route.fulfill({response:original,json:snapshot});
  });
  await page.goto('/');await page.getByLabel('Operator token').fill(token);await page.getByRole('button',{name:'Connect',exact:true}).click();await page.getByRole('button',{name:/Publisher observation fixture/}).click();
  await expect(page.locator('#merges')).toContainText('release-owner');await expect(page.locator('#merges')).toContainText('unknown');expect(await page.evaluate(()=>('mergeInjected' in window))).toBe(false);
  const card=page.locator('[data-publication="pub-view-fixture"]');await expect(card).toContainText('quarantined');await expect(card).toContainText('synthetic / no live authority');await expect(card).toContainText('Publication does not establish merge, deployment or recovery');expect(await page.evaluate(()=>('publicationInjected' in window))).toBe(false);
  await page.getByText('Submit versioned review command',{exact:true}).click();await page.locator('#review-command-kind').selectOption('publications');await page.reload();await expect(card).toContainText('Lost acknowledgment');expect(await page.evaluate(()=>('publicationInjected' in window))).toBe(false);
});
