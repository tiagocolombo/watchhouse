import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,existsSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,delimiter,resolve} from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
const token='browser-test-operator-token-32-bytes';
test('repair readiness preserves denied real execution and accountable escalation across reload',async({page,context})=>{
  const root=mkdtempSync(join(tmpdir(),'watchhouse-repair-'));const repo=join(root,'repo');const bases=join(root,'bases');mkdirSync(repo);mkdirSync(bases);
  try {
    execFileSync('git',['init','-q',repo]);writeFileSync(join(repo,'fixture.txt'),'fixture add(2,2)=5\n');execFileSync('git',['-C',repo,'add','.']);execFileSync('git',['-C',repo,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','Synthetic fixture']);
    const base=execFileSync('git',['-C',repo,'rev-parse','HEAD'],{encoding:'utf8'}).trim();
    const git=process.env.PATH!.split(delimiter).map(p=>join(p,process.platform==='win32'?'git.exe':'git')).find(p=>existsSync(p))!;
    const config={schema_version:1,revision:1,repository:'fixture/checkout',source_path:repo,base_commit:base,workspace_root:bases,runtime_path:resolve(git),runtime_digest:`sha256:${createHash('sha256').update(readFileSync(git)).digest('hex')}`,max_files:20,max_blob_bytes:65536,max_total_bytes:1048576,preparation_seconds:30,probe_seconds:5};
    const file=join(root,'config.json');writeFileSync(file,JSON.stringify(config));execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','repair-source','--file',file,'--operator','operator']);
    const auth={Authorization:`Bearer ${token}`};
    const created=await context.request.post('/api/v1/cases',{headers:auth,data:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Synthetic repair readiness fixture',title:'Repair preparation fixture',description:'No real harness is admitted'}});expect(created.status()).toBe(201);const id=(await created.json()).id;
    const snapshot=async()=> (await context.request.get(`/api/v1/cases/${id}`,{headers:auth})).json();await expect.poll(async()=>(await snapshot()).case.status).toBe('awaiting_review');let s=await snapshot();
    const evidence=await context.request.post(`/api/v1/cases/${id}/evidence`,{headers:auth,data:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Report fixture contract',expected_version:s.case.version,scope:'fixture checkout',statement:'Operator fixture report: 2+2 returned 5, expected 4',collection_method:'Manual synthetic inspection',observed_at:null,uncertainty:'No independently observed regression'}});expect(evidence.ok()).toBe(true);const digest=(await evidence.json()).evidence.digest;s=await snapshot();
    await page.goto('/');await page.getByLabel('Operator token').fill(token);await page.getByRole('button',{name:'Connect',exact:true}).click();await page.getByRole('button',{name:/Repair preparation fixture/}).click();await expect(page.locator('#repair-readiness')).toContainText('No repair requested');
    await page.getByText('Submit versioned review command',{exact:true}).click();await page.locator('#review-command-kind').selectOption('repairs');const command={command:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Prepare bounded repair'},expected_version:s.case.version},source_revision:1,objective:'<script>window.repairInjected=true</script> Correct the fixture',expected_behavior:'2+2 returns 4',regression_plan:'Fail on pinned base, pass on independently checked candidate',evidence_refs:[digest]};await page.locator('#review-command-json').fill(JSON.stringify(command));await page.getByRole('button',{name:'Submit review command',exact:true}).click();
    await expect(page.locator('#repair-readiness')).toContainText('blocked');await expect(page.locator('#repair-readiness')).toContainText('No admitted isolated worker');await expect(page.locator('#team-escalations')).toContainText('Repair is blocked');expect(await page.evaluate(()=>('repairInjected' in window))).toBe(false);
    const first=await snapshot();expect(first.repairs).toHaveLength(1);const retry=await context.request.post(`/api/v1/cases/${id}/repairs`,{headers:auth,data:command});expect(retry.ok()).toBe(true);expect((await snapshot()).repairs).toHaveLength(1);
    const trialAction=async(action:unknown)=>{const current=await snapshot();return {command:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'<script>window.trialInjected=true</script> Synthetic lifecycle drill'},expected_version:current.case.version},action};};
    await page.locator('#review-command-kind').selectOption('adapter-trials');
    const startTrial=await trialAction({type:'start',repair_id:first.repairs[0].id,scenario:'start_unknown',wall_seconds:300});
    await page.locator('#review-command-json').fill(JSON.stringify(startTrial));await page.getByRole('button',{name:'Submit review command',exact:true}).click();
    await expect(page.locator('#adapter-trials')).toContainText('unknown');await expect(page.locator('#adapter-trials')).toContainText('Reconcile the existing handle');
    const trial=(await snapshot()).adapter_trials[0];
    const repeat=await context.request.post(`/api/v1/cases/${id}/adapter-trials`,{headers:auth,data:startTrial});expect(repeat.ok()).toBe(true);expect((await snapshot()).adapter_trials).toHaveLength(1);
    const denied=await context.request.post(`/api/v1/cases/${id}/adapter-trials`,{data:startTrial});expect(denied.status()).toBe(401);
    for(const type of ['reconcile','advance']){const response=await context.request.post(`/api/v1/cases/${id}/adapter-trials`,{headers:auth,data:await trialAction({type,trial_id:trial.id})});expect(response.ok()).toBe(true);}
    await expect(page.locator('#adapter-trials')).toContainText('completed');await expect(page.locator('#adapter-trials')).toContainText('Real execution admitted: no');expect(await page.evaluate(()=>('trialInjected' in window))).toBe(false);
    const status=await context.request.get(`/api/v1/cases/${id}/adapter-trials/${trial.id}`,{headers:auth});expect(status.ok()).toBe(true);const result=await status.json();expect(result.result.synthetic).toBe(true);expect(result.result.actual_model).toBeNull();
    const stream=await context.request.get(`/api/v1/cases/${id}/adapter-trials/${trial.id}/events?after=2`,{headers:auth});expect(stream.ok()).toBe(true);expect((await stream.json()).map((e:{sequence:number})=>e.sequence)).toEqual([3,4]);
    const output=await context.request.get(`/api/v1/evidence/${encodeURIComponent(result.evidence_digest)}`,{headers:auth});expect(output.ok()).toBe(true);expect(await output.text()).toContain('fake-adapter-protocol-v1');
    const qualificationRoot=join(root,'qualification');mkdirSync(qualificationRoot);
    const pinned={path:resolve(git),digest:config.runtime_digest};const workerPolicy={schema_version:1,revision:1,boundary:pinned,limiter:pinned,probe:pinned,scratch_root:qualificationRoot,wall_seconds:2,cpu_seconds:1,memory_bytes:134217728,file_bytes:8192,descriptors:64,valid_seconds:60};const policyFile=join(root,'worker-policy.json');writeFileSync(policyFile,JSON.stringify(workerPolicy));execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','worker-policy','--file',policyFile,'--operator','operator']);
    s=await snapshot();const qualify={command:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'<script>window.qualificationInjected=true</script> Controlled fixture'},expected_version:s.case.version},policy_revision:1,source_revision:1};const commandFile=join(root,'qualify.json');writeFileSync(commandFile,JSON.stringify(qualify));const outcome=JSON.parse(execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','case','qualify-worker',id,'--file',commandFile],{encoding:'utf8'}));expect(outcome.state).toBe('failed');expect(outcome.real_execution_admitted).toBe(false);
    await expect(page.locator('#worker-qualification')).toContainText('failed');await expect(page.locator('#worker-qualification')).toContainText('Real execution admitted: no');expect(await page.evaluate(()=>('qualificationInjected' in window))).toBe(false);
    const report=await context.request.get(`/api/v1/cases/${id}/worker-qualification/markdown`,{headers:auth});expect(report.ok()).toBe(true);expect(await report.text()).toContain('hermetic-probe-v1');const replay=JSON.parse(execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','case','qualify-worker',id,'--file',commandFile],{encoding:'utf8'}));expect(replay.id).toBe(outcome.id);expect((await snapshot()).worker_qualifications).toHaveLength(1);
    const forbidden=await context.request.post(`/api/v1/cases/${id}/worker-qualification`,{headers:auth,data:qualify});expect(forbidden.status()).toBe(404);
    await page.reload();await expect(page.locator('#repair-readiness')).toContainText('blocked');await expect(page.locator('#adapter-trials')).toContainText('completed');await expect(page.locator('#worker-qualification')).toContainText('failed');await expect(page.locator('#owner')).toContainText('operator');await page.setViewportSize({width:390,height:844});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  } finally {rmSync(root,{recursive:true,force:true});}
});

test('native container commands show a blocked preflight without launching a workload',async({page,context})=>{
  const root=mkdtempSync(join(tmpdir(),'watchhouse-native-ui-'));
  try {
    const auth={Authorization:`Bearer ${token}`};
    const governing=JSON.parse(readFileSync('../policy/local-fake-v1.json','utf8'));governing.revision=2;governing.name='browser-explicit-v2';
    const governingFile=join(root,'governing.json');writeFileSync(governingFile,JSON.stringify(governing));execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','policy','--file',governingFile,'--operator','operator']);
    const git=process.env.PATH!.split(delimiter).map(p=>join(p,'git')).find(p=>existsSync(p))!;
    const digest=`sha256:${createHash('sha256').update(readFileSync(git)).digest('hex')}`;
    const configuration={schema_version:1,revision:1,docker_path:resolve(git),docker_digest:digest,socket:'/var/run/docker.sock',image:`sha256:${'0'.repeat(64)}`,runner_digest:digest,staging_root:root,program:'/watchhouse-worker-init',args:['fixture-regression'],memory_bytes:268435456,pids:32,cpu_micros:50000,work_bytes:16777216,wall_seconds:10,output_bytes:4096,proof_seconds:300,dispatch_grace_seconds:90,cleanup_attempts:3,cleanup_retry_seconds:2};
    const file=join(root,'container.json');writeFileSync(file,JSON.stringify(configuration));execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','container-policy','--file',file,'--operator','operator']);
    const created=await context.request.post('/api/v1/cases',{headers:auth,data:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'Native preflight fixture',title:'Blocked native preflight',description:'Git is deliberately not a Docker engine; no workload can start'}});expect(created.status()).toBe(201);const id=(await created.json()).id;
    const snapshot=async()=> (await context.request.get(`/api/v1/cases/${id}`,{headers:auth})).json();await expect.poll(async()=>(await snapshot()).case.status).toBe('awaiting_review');
    await page.goto('/');await page.getByLabel('Operator token').fill(token);await page.getByRole('button',{name:'Connect',exact:true}).click();await page.getByRole('button',{name:/Blocked native preflight/}).click();
    const state=await snapshot();const command={command:{meta:{schema_version:1,idempotency_key:randomUUID(),actor:'operator',reason:'<script>window.nativeInjected=true</script> Native preflight fixture'},expected_version:state.case.version},policy_revision:1};
    await page.getByText('Submit versioned review command',{exact:true}).click();await page.locator('#review-command-kind').selectOption('container-qualification');await page.locator('#review-command-json').fill(JSON.stringify(command));await page.getByRole('button',{name:'Submit review command',exact:true}).click();
    await expect(page.locator('#native-repair')).toContainText('qualification · queued');const queued=(await snapshot()).container_jobs[0];
    execFileSync('../target/debug/watchhouse',['--db','test-results/browser.db','container-worker','--once']);
    await expect(page.locator('#native-repair')).toContainText('qualification · blocked');await expect(page.locator('#native-repair')).toContainText('before effect dispatch');await expect(page.locator('#team-escalations')).toContainText('native execution');
    const terminal=(await snapshot()).container_jobs[0];expect(terminal.create_dispatched).toBe(false);expect(terminal.quarantined).toBe(false);expect(terminal.result).toBeNull();
    const retry=await context.request.post(`/api/v1/cases/${id}/container-qualification`,{headers:auth,data:command});expect(retry.ok()).toBe(true);expect((await retry.json()).id).toBe(queued.id);expect((await snapshot()).container_jobs).toHaveLength(1);
    await page.reload();await expect(page.locator('#native-repair')).toContainText('qualification · blocked');expect(await page.evaluate(()=>('nativeInjected' in window))).toBe(false);await page.setViewportSize({width:390,height:844});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  } finally {rmSync(root,{recursive:true,force:true});}
});
