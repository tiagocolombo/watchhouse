import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
test('OS backup audit is visible as inert factory metadata without exposing backup bytes',async({page,context})=>{
 const parent=mkdtempSync(join(tmpdir(),'watchhouse-browser-backup-'));const output=join(parent,'snapshot');
 const reason='Synthetic backup <img src=x onerror=alert(1)>; no live verification';
 try {
  const report=JSON.parse(execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','backup','--output',output,'--operator','operator','--reason',reason],{encoding:'utf8'}));
  expect(report.restore_available).toBe(false);
  const checked=JSON.parse(execFileSync('../target/debug/watchhouse',['backup-inspect','--directory',output,'--manifest-digest',report.manifest_digest],{encoding:'utf8'}));expect(checked.id).toBe(report.id);
  const r=await context.request.get('/api/v1/factory',{headers:{Authorization:'Bearer browser-test-operator-token-32-bytes'}});const status=await r.json();expect(status.audit.some((e:{backup_id?:string})=>e.backup_id===report.id)).toBe(true);
  await page.goto('/');await page.getByLabel('Operator token').fill('browser-test-operator-token-32-bytes');await page.getByRole('button',{name:'Connect',exact:true}).click();await page.getByText('Factory health and control history',{exact:true}).click();
  await expect(page.locator('#factory-details')).toContainText(report.manifest_digest);await expect(page.locator('#factory-details')).toContainText(reason);await expect(page.locator('#factory-details img')).toHaveCount(0);await expect(page.locator('#factory-details')).not.toContainText(output);
 } finally {rmSync(parent,{recursive:true,force:true});}
});
