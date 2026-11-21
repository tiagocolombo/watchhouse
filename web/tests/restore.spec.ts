import {test,expect} from '@playwright/test';
import {execFileSync,spawn} from 'node:child_process';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
test('restored coordinator stays visibly isolated after resume and reload',async({page,context})=>{
 const root=mkdtempSync(join(tmpdir(),'watchhouse-browser-restore-'));const source=join(root,'source.db');const backup=join(root,'backup');const output=join(root,'restored');const binary='../target/debug/watchhouse';
 const cli=(args:string[])=>JSON.parse(execFileSync(binary,args,{encoding:'utf8'}));const reason='Browser recovery <img src=x onerror=alert(1)>; synthetic data only';
 let child:ReturnType<typeof spawn>|undefined;
 try {
  cli(['--db',source,'case','create','--title','Restored browser fixture','--key',randomUUID()]);
  const b=cli(['--db',source,'backup','--output',backup,'--operator','operator','--reason','Synthetic browser restore input']);const r=cli(['restore','--backup',backup,'--output',output,'--manifest-digest',b.manifest_digest,'--operator','operator','--reason',reason]);
  const env:NodeJS.ProcessEnv={WATCHHOUSE_TOKEN:'restored-browser-operator-token-32-bytes'};for(const key of ['SystemRoot','WINDIR'])if(process.env[key])env[key]=process.env[key];
  child=spawn(binary,['--db',join(output,'watchhouse.db'),'serve','--port','0','--manual-workers'],{env,stdio:['ignore','ignore','pipe']});
  const address=await new Promise<string>((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(new Error('Restored coordinator startup timed out: '+log)),10000);child!.once('error',e=>{clearTimeout(timer);reject(e);});child!.once('exit',code=>{clearTimeout(timer);reject(new Error('Restored coordinator exited '+code+': '+log));});child!.stderr!.on('data',chunk=>{log=(log+chunk.toString()).slice(-8192);const m=log.match(/Watchhouse: (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearTimeout(timer);resolve(m[1]);}});});
  await page.goto(address);await page.getByLabel('Operator token').fill(env.WATCHHOUSE_TOKEN!);await page.getByRole('button',{name:'Connect',exact:true}).click();await expect(page.locator('#factory-status')).toContainText('copied authority remains held');await expect(page.locator('#factory-pauses')).toContainText('Restored coordinator');
  await page.getByText('Factory health and control history',{exact:true}).click();await expect(page.locator('#factory-details')).toContainText(r.id);await expect(page.locator('#factory-details')).toContainText(reason);await expect(page.locator('#factory-details img')).toHaveCount(0);
  await page.getByRole('button',{name:/Restored browser fixture/}).click();await expect(page.locator('#case-status')).toHaveText('queued');await page.locator('#factory-pauses').getByRole('button',{name:'Resume this pause',exact:true}).click();await expect(page.locator('#factory-pauses')).toBeEmpty();await expect(page.locator('#factory-status')).toContainText('copied authority remains held');
  await page.reload();await expect(page.locator('#factory-status')).toContainText('copied authority remains held');await expect(page.locator('#case-status')).toHaveText('queued');
  expect((await context.request.get(address+'/api/v1/factory')).status()).toBe(401);
 } finally {
  if(child&&child.exitCode===null&&child.signalCode===null){const exited=new Promise<void>(resolve=>child!.once('exit',()=>resolve()));child.kill();await exited;}
  rmSync(root,{recursive:true,force:true});
 }
});
