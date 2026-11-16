interface Case { id:string; title:string; description:string; owner:string; status:string; version:number }
interface TimelineEvent { cursor:number; event_id:string; type:string; recorded_at:string; actor:{kind:string;id:string}; payload:{reason:string;evidence_refs?:string[]} }
interface QualificationPlan { kind:string; after_seconds:number; recovery_indicators?:string; collection_task?:string }
interface Disposition { id:string; state:string; occurrence_count:number; recorded_at:string; due_at:string; record:{owner:string;scope:string;uncertainty:string;reason:string;evidence_refs:string[];reconsideration_conditions:string;reopen_after_occurrences:number;plan:QualificationPlan} }
interface Obligation { id:string; owner:string; task:string; due_at:string; state:string; triggered_at:string|null }
interface EvidenceReference { digest:string; provenance:string; recorded_at:string; summary:string }
interface Qualification { dispositions:Disposition[]; obligations:Obligation[]; evidence:EvidenceReference[] }
interface TeamMessage {id:string;kind:string;body:string;sender:{kind:string;id:string};recipients:string[];recorded_at:string;acknowledge_by:string|null;acknowledged_by:string[];evidence_refs:string[]}
interface TeamBlock {id:string;state:string;owner:string;response_due_at:string;checkpoint_digest:string;escalation_path:string;details:{reason_code:string;assistance_needed:string;uncertainty:string;last_useful_progress:string}}
interface TeamHandoff {id:string;state:string;previous_owner:string;successor:string;next_owner:string;acknowledge_by:string;briefing:{remaining_attempts:number;overall_deadline:string;checkpoint_digest:string|null;message_refs:string[]}}
interface TeamEscalation {id:string;state:string;owner:string;reason:string;next_action:string}
interface Team {control:{state:string;executor:string;overall_deadline:string|null;max_attempts:number}|null;messages:TeamMessage[];blocks:TeamBlock[];handoffs:TeamHandoff[];escalations:TeamEscalation[];runs:{run_id:string;state:string;executor:string;progress_deadline:string;heartbeat_at:string;progress_at:string}[];late_results:{run_id:string;digest:string}[]}
interface SourceFreshness {source_key:string;state:string;age_seconds:number|null;last_success_at:string|null;error:string|null}
interface Correlations {relations:{source_case:string;target_case:string;kind:string;active:boolean;context_current:boolean;reason:string;revision:number}[];candidates:{case_id:string;reason:string}[];limitation:string}
interface RepairRequest {id:string;state:string;owner:string;repository:string;base_commit:string;denials:string[];next_action:string}
interface AdapterTrial {id:string;state:string;scope:string;reason:string;sequence:number;pending_operation:string|null;evidence_digest:string|null;envelope:{owner:string;deadline:string}}
interface WorkerQualification {id:string;state:string;scope:string;owner:string;reason:string;real_execution_admitted:boolean;verified:string[];time_current:boolean;expires_at:string|null;quarantined:boolean;evidence_digest:string|null;reconciliation:{operator:string;reason:string}|null}
interface Snapshot { case:Case; events:TimelineEvent[]; cursor:number; results:{summary:string;conclusion:string}[]; qualification:Qualification; team:Team; challenges:unknown[]; gate:{validated_version:number;closed_version:number|null}|null; policy:unknown; notifications:unknown[]; source:unknown; source_freshness:SourceFreshness|null; incident:unknown; incident_operations:unknown; incident_role_offers:unknown[]; incident_artifacts:unknown[]; correlations:Correlations; repairs:RepairRequest[]; worker_qualifications:WorkerQualification[]; adapter_trials:AdapterTrial[] }
const el = <T extends HTMLElement = HTMLElement>(id:string) => document.getElementById(id) as T;
const input = (id:string) => el<HTMLInputElement>(id).value;
let token = sessionStorage.getItem('watchhouse-token') ?? '';
let selected:Case | undefined;
let selectedSourceKey="";
let healthPolling=false;
let qualification:Qualification = {dispositions:[],obligations:[],evidence:[]};
let evidenceCase = "";
let team:Team|undefined;
let generation = 0;
let active:AbortController | undefined;
let createKey = crypto.randomUUID();
let createPayload = '';
const seen = new Set<string>();
function fail(error:unknown) { el('error').textContent = error instanceof Error ? error.message : String(error); }
async function api<T>(path:string,body?:unknown):Promise<T> {
  const response = await fetch(`/api/v1${path}`,{method:body?'POST':'GET',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});
  if (!response.ok) { let message = await response.text(); try { message = JSON.parse(message).message ?? message; } catch { /* non-JSON protocol error */ } throw new HttpError(message,response.status); }
  return response.json() as Promise<T>;
}
async function queue() {
  const cases = await api<Case[]>('/cases');
  el('source-health').textContent=JSON.stringify(await api('/sources/sentry'),null,2);
  el('cases').replaceChildren();
  for (const item of cases) {
    const button = document.createElement('button'); button.textContent = `${item.title} · ${item.status.replaceAll('_',' ')}`;
    button.setAttribute('aria-current',String(selected?.id===item.id)); button.onclick = () => void openCase(item.id).catch(fail); el('cases').append(button);
  }
  if (!cases.length) el('cases').textContent = 'No cases yet.';
}
function renderEvent(event:TimelineEvent) {
  if (seen.has(event.event_id)) return; seen.add(event.event_id);
  const li = document.createElement('li'); li.dataset.eventId=event.event_id;
  const title=document.createElement('strong'); title.textContent=event.type;
  const time=document.createElement('small'); time.textContent=`${new Date(event.recorded_at).toLocaleString()} · ${event.actor.id}`;
  const reason=document.createElement('p'); reason.textContent=event.payload.reason;
  li.append(title,reason,time);
  for(const digest of event.payload.evidence_refs ?? []) {
    const link=document.createElement('a'); link.href=`/api/v1/evidence/${digest}`; link.textContent=' · Inspect persisted evidence';
    link.onclick=e=>{e.preventDefault(); void api<unknown>(`/evidence/${digest}`).then(value=>{const pre=document.createElement('pre'); pre.textContent=JSON.stringify(value,null,2); pre.style.whiteSpace='pre-wrap'; li.append(pre);link.remove();}).catch(fail);};
    li.append(link);
  }
  el('timeline').append(li);
}
function renderState(snapshot:Snapshot) {
  selected=snapshot.case;
  selectedSourceKey=snapshot.source_freshness?.source_key??"";
  renderFreshness(snapshot.source_freshness);
  el('case-id').textContent=selected.id; el('case-title').textContent=selected.title;
  el('case-status').textContent=selected.status.replaceAll('_',' ');
  el('version').textContent=`Version ${selected.version}`; el('owner').textContent=`Owner: ${selected.owner}`;
  el('case-description').textContent=selected.description; el<HTMLInputElement>('rename-title').value=selected.title;
  el('next').textContent=selected.status==='awaiting_review'?`Next: ${selected.owner} owns review of the synthetic result and the next investigation. Case remains open.`:selected.status==='running'?'Next: fake investigator returns a bounded result. Operator remains accountable.':'Next: coordinator claims the queued investigation. Operator remains accountable.';
  el('results').replaceChildren();
  for(const result of snapshot.results) { const p=document.createElement('p');p.textContent=result.summary; el('results').append(p); }
  if (!snapshot.results.length) el('results').textContent='No result recorded yet.';
  renderRepairReadiness(snapshot.repairs);
  renderWorkerQualification(snapshot.worker_qualifications);
  renderAdapterTrials(snapshot.adapter_trials);
  renderCorrelations(snapshot.correlations);
  renderQualification(snapshot);
  renderTeam(snapshot);
  el('challenge-records').textContent=JSON.stringify({correlations:snapshot.correlations,incident_artifacts:snapshot.incident_artifacts,incident_operations:snapshot.incident_operations,incident_role_offers:snapshot.incident_role_offers,source:snapshot.source,source_freshness:snapshot.source_freshness,incident:snapshot.incident,notifications:snapshot.notifications,policy:snapshot.policy,challenges:snapshot.challenges,gate:snapshot.gate,gateCurrent:snapshot.gate?.closed_version===snapshot.case.version},null,2);
}
async function follow(id:string,cursor:number,current:number,signal:AbortSignal) {
  while(!signal.aborted && current===generation) {
    try {
      el('connection').textContent='Timeline connecting…';
      const fresh=await api<Snapshot>(`/cases/${id}`);
      if(current!==generation || signal.aborted) return;
      renderState(fresh); fresh.events.forEach(renderEvent); cursor=Math.max(cursor,fresh.cursor);
      const response=await fetch(`/api/v1/cases/${id}/events?after=${cursor}`,{headers:{Authorization:`Bearer ${token}`,'Last-Event-ID':String(cursor)},signal});
      if(!response.ok || !response.body) throw new Error('Timeline unavailable');
      el('connection').textContent='Timeline live';
      const reader=response.body.pipeThrough(new TextDecoderStream()).getReader(); let buffer='';
      try {
        while(!signal.aborted) {
          const {done,value}=await reader.read(); if(done) break; buffer+=value;
          let end:number;
          while((end=buffer.indexOf('\n\n'))>=0) {
            const frame=buffer.slice(0,end); buffer=buffer.slice(end+2);
            if(frame.includes('event: unavailable')) throw new Error('Timeline requires reconnect');
            const data=frame.split('\n').find(line=>line.startsWith('data: ')); if(!data) continue;
            if(current!==generation) return;
            const event=JSON.parse(data.slice(6)) as TimelineEvent;
            renderEvent(event); cursor=event.cursor;
            const snapshot=await api<Snapshot>(`/cases/${id}`);
            if(current!==generation) return;
            renderState(snapshot); await queue();
          }
        }
      } finally { await reader.cancel().catch(()=>{}); }
    } catch(error) { if(signal.aborted) return; el('connection').textContent='Reconnecting timeline…'; }
    await new Promise(resolve=>setTimeout(resolve,500));
  }
}
async function openCase(id:string) {
  active?.abort(); const current=++generation; active=new AbortController();
  const snapshot=await api<Snapshot>(`/cases/${id}`); if(current!==generation) return;
  el('empty').hidden=true;el('case-view').hidden=false;el('timeline').replaceChildren();seen.clear();
  renderState(snapshot); snapshot.events.forEach(renderEvent);location.hash=id;
  void follow(id,snapshot.cursor,current,active.signal);await queue();
}
async function connect() {
  await queue(); sessionStorage.setItem('watchhouse-token',token);el('login').hidden=true;el('workspace').hidden=false;el('error').textContent='';el('connection').textContent='Workspace connected';
  if(location.hash.startsWith('#case_')) await openCase(location.hash.slice(1));
}
el('login-form').onsubmit=e=>{e.preventDefault();token=input('token');void connect().catch(fail);};
el('create-form').onsubmit=e=>{e.preventDefault();void(async()=>{
  el<HTMLButtonElement>('create').disabled=true;el('error').textContent='';
  const payload={schema_version:1,actor:'operator',reason:input('reason'),title:input('title'),description:input('description')};
  // Retain a key after an uncertain response. Changed content is a new command.
  if(createPayload!==JSON.stringify(payload)){createKey=crypto.randomUUID();createPayload=JSON.stringify(payload);}
  try { const item=await api<Case>('/cases',{...payload,idempotency_key:createKey});createKey=crypto.randomUUID();createPayload='';el<HTMLFormElement>('create-form').reset();await openCase(item.id); }
  finally {el<HTMLButtonElement>('create').disabled=false;}
})().catch(fail);};
el('rename-form').onsubmit=e=>{e.preventDefault();if(!selected) return; const item=selected;void api<Case>(`/cases/${item.id}/rename`,{schema_version:1,idempotency_key:crypto.randomUUID(),actor:'operator',reason:input('rename-reason'),title:input('rename-title'),expected_version:item.version}).then(()=>openCase(item.id)).catch(fail);};


class HttpError extends Error {
  constructor(message:string,public readonly status:number) {super(message);}
}
const pendingCommands = new Map<string,{signature:string;body:Record<string,unknown>}>();
async function command<T>(id:string,action:string,fields:Record<string,unknown>,expectedVersion:number):Promise<T> {
  if(!selected || selected.id!==id) throw new Error('Select the case again before submitting');
  const key=`${id}/${action}`;
  const signature=JSON.stringify(fields);
  let pending=pendingCommands.get(key);
  if(!pending || pending.signature!==signature) {
    const idempotency_key=crypto.randomUUID();
    const body=action.startsWith('team/') ? (()=>{
      const {reason,...data}=fields;
      return {command:{meta:{schema_version:1,idempotency_key,actor:'operator',reason},expected_version:expectedVersion},...data};
    })() : {schema_version:1,idempotency_key,actor:'operator',expected_version:expectedVersion,...fields};
    pending={signature,body};
    pendingCommands.set(key,pending);
  }
  try {
    const result=await api<T>(`/cases/${id}/${action}`,pending.body);
    pendingCommands.delete(key);return result;
  } catch(error) {
    // Retain the exact key and version only after an uncertain network response.
    if(error instanceof HttpError) pendingCommands.delete(key);
    throw error;
  }
}
function paragraph(parent:HTMLElement,text:string) {
  const p=document.createElement('p');p.textContent=text;parent.append(p);
}
function renderQualification(snapshot:Snapshot) {
  qualification=snapshot.qualification;
  const latest=qualification.dispositions.at(-1);
  const panel=el('qualification-state');panel.replaceChildren();
  if(latest) {
    const name=latest.record.plan.kind.replaceAll('_',' ');
    paragraph(panel,`${name} · ${latest.state} · ${latest.occurrence_count}/${latest.record.reopen_after_occurrences} recurrences`);
    paragraph(panel,`Scope: ${latest.record.scope}`);
    paragraph(panel,`Decision: ${latest.record.reason}`);
    paragraph(panel,`Uncertainty: ${latest.record.uncertainty}`);
    paragraph(panel,`Reconsider when: ${latest.record.reconsideration_conditions}`);
    paragraph(panel,`Evidence: ${latest.record.evidence_refs.join(', ')}`);
    if(latest.state==='reopened') el('next').textContent=`Next: ${latest.record.owner} must reassess the reopened case. No repair or recovery is implied.`;
    else el('next').textContent=`Next: ${latest.record.owner} owns the scheduled review. This disposition applies only to its recorded scope.`;
    for(const older of qualification.dispositions.slice(0,-1)) {
      const details=document.createElement('details');const summary=document.createElement('summary');
      summary.textContent=`Prior decision: ${older.record.plan.kind.replaceAll('_',' ')} · ${older.state}`;
      details.append(summary);paragraph(details,`${older.record.reason} · Scope: ${older.record.scope} · Uncertainty: ${older.record.uncertainty}`);panel.append(details);
    }
  } else paragraph(panel,'No disposition recorded. Operator review is required.');
  const obligations=el('obligations');obligations.replaceChildren();
  for(const item of qualification.obligations.filter(o=>o.state!=='superseded')) {
    const box=document.createElement('div');if(item.state==='due') box.className='obligation-due';
    paragraph(box,`${item.state==='due'?'DUE — review required':'Scheduled'} · Owner: ${item.owner}`);
    paragraph(box,item.task);paragraph(box,`Original deadline: ${new Date(item.due_at).toLocaleString()}`);
    if(item.triggered_at) paragraph(box,`Reopened at: ${new Date(item.triggered_at).toLocaleString()}`);
    obligations.append(box);
  }
  const previous=new Map(Array.from(el('evidence-refs').querySelectorAll<HTMLInputElement>('input')).map(c=>[c.value,c.checked]));
  if(evidenceCase!==snapshot.case.id) {
    previous.clear();evidenceCase=snapshot.case.id;formVersions.clear();
    for(const id of ['evidence-form','disposition-form','reconsider-form','team-message-form','handoff-form']) el<HTMLFormElement>(id).reset();
    el<HTMLInputElement>('disposition-owner').value=snapshot.case.owner;el<HTMLInputElement>('handoff-owner').value=snapshot.case.owner;planFields();
  }
  const refs=el('evidence-refs');refs.replaceChildren();
  for(const evidence of qualification.evidence) {
    const label=document.createElement('label');const checkbox=document.createElement('input');checkbox.type='checkbox';checkbox.value=evidence.digest;
    checkbox.checked=previous.get(evidence.digest)??true;
    label.append(checkbox,document.createTextNode(`${({'operator-reported-v1':'Operator report','fake-v1:synthetic-result':'Synthetic result','fake-worker-progress-v1':'Reported fake progress','fake-worker-checkpoint-v1':'Fake worker checkpoint','fake-v1:late-unaccepted':'Unaccepted late result'} as Record<string,string>)[evidence.provenance]??'Evidence'}: ${evidence.summary} (${evidence.digest.slice(0,19)}…)`));refs.append(label);
  }
  el<HTMLButtonElement>('record-disposition').disabled=snapshot.case.status!=='awaiting_review';
  el('reconsider-panel').hidden=!latest || latest.state!=='active';
}
function chosenEvidence():string[] {
  return Array.from(el('evidence-refs').querySelectorAll<HTMLInputElement>('input:checked')).map(c=>c.value);
}
function planFields() {
  const kind=input('plan-kind');
  el('collection-label').hidden=kind!=='insufficient_evidence';
  el('indicators-label').hidden=kind!=='apparently_transient';
  el<HTMLInputElement>('disposition-collection').required=kind==='insufficient_evidence';
  el<HTMLInputElement>('disposition-indicators').required=kind==='apparently_transient';
}
el('plan-kind').onchange=planFields;
const formVersions = new Map<string,number>();
function handleForm(id:string,submit:(caseId:string,version:number)=>Promise<unknown>) {
  el(id).addEventListener('focusin',()=>{
    if(selected && !formVersions.has(id)) formVersions.set(id,selected.version);
  });
  el<HTMLFormElement>(id).onsubmit=e=>{
    e.preventDefault();if(!selected) return;const caseId=selected.id;
    const button=el(id).querySelector<HTMLButtonElement>('button')!;button.disabled=true;el('error').textContent='';
    void submit(caseId,formVersions.get(id)??selected.version).then(()=>{formVersions.delete(id);if(selected?.id===caseId) return openCase(caseId);}).catch(fail).finally(()=>{button.disabled=false;});
  };
}
handleForm('evidence-form',(id,version)=>command(id,'evidence',{
  scope:input('evidence-scope'),statement:input('evidence-statement'),collection_method:input('evidence-method'),
  observed_at:input('evidence-time')||null,uncertainty:input('evidence-uncertainty'),reason:input('evidence-reason'),
},version));
handleForm('disposition-form',(id,version)=>{
  const plan:QualificationPlan={kind:input('plan-kind'),after_seconds:Number(input('disposition-seconds'))};
  if(plan.kind==='apparently_transient') plan.recovery_indicators=input('disposition-indicators');
  if(plan.kind==='insufficient_evidence') plan.collection_task=input('disposition-collection');
  return command(id,'dispositions',{
    reason:input('disposition-reason'),owner:input('disposition-owner'),scope:input('disposition-scope'),uncertainty:input('disposition-uncertainty'),
    evidence_refs:chosenEvidence(),reconsideration_conditions:input('disposition-conditions'),reopen_after_occurrences:Number(input('disposition-threshold')),plan,
  },version);
});
handleForm('reconsider-form',(id,version)=>command(id,'reconsider',{
  reason:input('reconsider-reason'),disposition_id:qualification.dispositions.at(-1)?.id,trigger:input('reconsider-trigger'),evidence_refs:chosenEvidence(),
},version));

function inspect(parent:HTMLElement,digest:string) {
  const button=document.createElement('button');button.type='button';button.textContent='Inspect checkpoint / evidence';
  button.onclick=()=>void api<unknown>(`/evidence/${digest}`).then(value=>{
    const pre=document.createElement('pre');pre.textContent=JSON.stringify(value,null,2);pre.style.whiteSpace='pre-wrap';parent.append(pre);button.remove();
  }).catch(fail);parent.append(button);
}
function renderTeam(snapshot:Snapshot) {
  team=snapshot.team;
  const panel=el('team-state');panel.replaceChildren();
  const control=team.control;
  if(control) {
    paragraph(panel,`${control.state.replaceAll('_',' ')} · Executor: ${control.executor} · Accountable owner: ${snapshot.case.owner}`);
    if(control.overall_deadline) paragraph(panel,`Assignment deadline: ${new Date(control.overall_deadline).toLocaleString()} · Attempt limit: ${control.max_attempts}`);
    const running=team.runs.find(r=>r.state==='running');
    if(running) paragraph(panel,`Last heartbeat: ${new Date(running.heartbeat_at).toLocaleString()} · Last useful progress: ${new Date(running.progress_at).toLocaleString()} · Progress due: ${new Date(running.progress_deadline).toLocaleString()}`);
    if(control.state==='blocked') el('next').textContent=`Next: ${snapshot.case.owner} must respond to the blocker, arrange an acknowledged handoff, or review the local escalation inbox.`;
    if(control.state==='handoff_pending') el('next').textContent=`Next: successor must accept the handoff. ${snapshot.case.owner} remains accountable.`;
  }
  const blocks=el('team-blocks');blocks.replaceChildren();
  for(const b of team.blocks) {
    const box=document.createElement('div');box.className='team-card';
    paragraph(box,`Block ${b.state} · ${b.details.reason_code.replaceAll('_',' ')} · Owner: ${b.owner}`);
    paragraph(box,b.details.assistance_needed);paragraph(box,`Last useful progress: ${b.details.last_useful_progress}`);
    paragraph(box,`Uncertainty: ${b.details.uncertainty}`);paragraph(box,`Response by ${new Date(b.response_due_at).toLocaleString()} · Escalation: ${b.escalation_path}`);
    inspect(box,b.checkpoint_digest);blocks.append(box);
  }
  const handoffs=el('team-handoffs');handoffs.replaceChildren();
  for(const h of team.handoffs) {
    const box=document.createElement('div');box.className='team-card';
    paragraph(box,`Handoff ${h.state} · Successor: ${h.successor} · Owner after acceptance: ${h.next_owner}`);
    paragraph(box,`Previous owner: ${h.previous_owner} · Accept by ${new Date(h.acknowledge_by).toLocaleString()}`);
    paragraph(box,`Frozen briefing: ${h.briefing.message_refs.length} relevant messages, ${h.briefing.remaining_attempts} available attempts at offer. Original assignment deadline: ${new Date(h.briefing.overall_deadline).toLocaleString()}`);
    if(h.briefing.checkpoint_digest) inspect(box,h.briefing.checkpoint_digest);handoffs.append(box);
  }
  const escalations=el('team-escalations');escalations.replaceChildren();
  for(const e of team.escalations) {
    const box=document.createElement('div');box.className='obligation-due';
    paragraph(box,`Local escalation ${e.state} · Owner: ${e.owner}`);paragraph(box,e.reason);paragraph(box,e.next_action);
    if(e.state==='open' && e.owner==='operator') {
      const button=document.createElement('button');button.textContent='Acknowledge local escalation';
      button.onclick=()=>{if(!selected) return;const id=selected.id;void command(id,'team/escalation-ack',{escalation_id:e.id,reason:'Operator acknowledged the local escalation; resolution remains outstanding'},selected.version).then(()=>openCase(id)).catch(fail);};box.append(button);
    } escalations.append(box);
  }
  const messages=el('team-messages');messages.replaceChildren();
  if(team.messages.length) {const title=document.createElement('h3');title.textContent='Durable team messages';messages.append(title);}
  for(const m of team.messages) {
    const box=document.createElement('article');box.className='team-card';box.dataset.messageId=m.id;
    paragraph(box,`${m.kind} · ${m.sender.id} → ${m.recipients.join(', ')}`);paragraph(box,m.body);
    paragraph(box,`Recorded ${new Date(m.recorded_at).toLocaleString()} · ${m.id}`);
    if(m.acknowledge_by) paragraph(box,`Acknowledge by ${new Date(m.acknowledge_by).toLocaleString()} · Acknowledged: ${m.acknowledged_by.join(', ')||'none'}`);
    if(m.recipients.includes('operator')&&!m.acknowledged_by.includes('operator')) {
      const button=document.createElement('button');button.textContent='Acknowledge message';button.onclick=()=>{if(!selected)return;const id=selected.id;void command(id,'team/message-ack',{message_id:m.id,reason:'Local operator read the directed message'},selected.version).then(()=>openCase(id)).catch(fail);};box.append(button);
    }
    messages.append(box);
  }
  for(const result of team.late_results) {
    const box=document.createElement('div');paragraph(box,`Late result from ${result.run_id} retained as unaccepted evidence. It did not advance workflow.`);inspect(box,result.digest);messages.append(box);
  }
  el('handoff-panel').hidden=control?.state!=='blocked'||!team.blocks.some(b=>b.state==='open'||b.state==='escalated');
}
handleForm('team-message-form',(id,version)=>command(id,'team/messages',{
  kind:input('team-kind'),body:input('team-body'),recipients:input('team-recipients').split(',').map(s=>s.trim()).filter(Boolean),
  evidence_refs:[],reply_to:input('team-reply')||null,ack_within_seconds:input('team-ack-seconds')?Number(input('team-ack-seconds')):null,reason:input('team-message-reason'),
},version));
handleForm('handoff-form',(id,version)=>command(id,'team/handoffs',{
  block_id:team?.blocks.filter(b=>b.state==='open'||b.state==='escalated').at(-1)?.id,successor:input('handoff-successor'),next_owner:input('handoff-owner'),
  acknowledge_within_seconds:Number(input('handoff-seconds')),reason:input('handoff-reason'),
},version));
if(token) void connect().catch(fail);

el<HTMLFormElement>('challenge-command').onsubmit=e=>{e.preventDefault();if(!selected)return;const id=selected.id;void api(`/cases/${id}/${input('review-command-kind')}`,JSON.parse(input('review-command-json'))).then(()=>openCase(id)).catch(fail);};

function renderFreshness(value:SourceFreshness|null) {
  el('source-freshness-summary').textContent=value?`Source read: ${value.state.replaceAll('_',' ')} · ${value.age_seconds===null?'No successful API read tracked':`Last successful API read ${value.age_seconds}s ago`} · ${value.error??'Source status does not establish service recovery'}`:'';
}
setInterval(()=>{if(!token||healthPolling||!el('login').hidden)return;healthPolling=true;
  void api<{known_issue_freshness:SourceFreshness[]}>('/sources/sentry').then(health=>{el('source-health').textContent=JSON.stringify(health,null,2);const value=health.known_issue_freshness.find(v=>v.source_key===selectedSourceKey);if(value)renderFreshness(value);}).catch(()=>{if(selectedSourceKey)el('source-freshness-summary').textContent='Source health unavailable; retained observations may be stale.';}).finally(()=>{healthPolling=false;});
},5000);

function renderCorrelations(value:Correlations) {
  el('correlation-limits').textContent=value.limitation;
  const links=el('correlation-links');links.replaceChildren();
  for(const r of value.relations) {
    const li=document.createElement('li');li.textContent=`${r.kind} · revision ${r.revision} · ${!r.active?'revoked':r.context_current?'reviewed fact snapshots unchanged':'source facts changed; review required'} · ${r.source_case} → ${r.target_case} · ${r.reason}`;links.append(li);
  }
  if(!value.relations.length) links.textContent='No recorded links.';
  const candidates=el('correlation-candidates');candidates.replaceChildren();
  for(const c of value.candidates) {
    const li=document.createElement('li');const button=document.createElement('button');button.textContent=`Compare case ${c.case_id}`;button.onclick=()=>{void openCase(c.case_id).catch(fail);};li.append(button);const text=document.createElement('p');text.textContent=c.reason;li.append(text);candidates.append(li);
  }
  if(!value.candidates.length) candidates.textContent='No current suggestions in the bounded scan. This does not establish uniqueness.';
}
el<HTMLButtonElement>('refresh-correlations').onclick=()=>{if(selected)void openCase(selected.id).catch(fail);};

function renderRepairReadiness(requests:RepairRequest[]) {
  const panel=el('repair-readiness');panel.replaceChildren();
  if(!requests.length) {panel.textContent='No repair requested. Real worker execution is not admitted.';return;}
  for(const r of requests) {const card=document.createElement('article');card.className='team-card';paragraph(card,`${r.state} · ${r.repository} @ ${r.base_commit} · Owner: ${r.owner}`);for(const denial of r.denials)paragraph(card,denial);paragraph(card,r.next_action);panel.append(card);}
}

function renderWorkerQualification(records:WorkerQualification[]) {
  const panel=el('worker-qualification');panel.replaceChildren();
  if(!records.length) {panel.textContent='No worker qualification observed. Real harness execution remains denied.';return;}
  for(const q of records) {const card=document.createElement('article');card.className='team-card';paragraph(card,`${q.state} · ${q.scope} · Owner: ${q.owner}`);paragraph(card,q.reason);paragraph(card,`Observed checks: ${q.verified.join(', ')||'none verified'} · Currency when last checked: ${q.time_current?'yes':'no'} · Real execution admitted: no`);if(q.reconciliation)paragraph(card,`Operator-reported cleanup: ${q.reconciliation.operator} · ${q.reconciliation.reason}. This is not an isolation pass.`);if(q.evidence_digest)inspect(card,q.evidence_digest);if(q.quarantined)paragraph(card,'Probe resources are quarantined; the operator must reconcile them. Retries do not launch another probe.');panel.append(card);}
}

el<HTMLButtonElement>('refresh-worker-status').onclick=()=>{if(selected)void openCase(selected.id).catch(fail);};

function renderAdapterTrials(records:AdapterTrial[]) {
  const panel=el('adapter-trials');panel.replaceChildren();
  if(!records.length){panel.textContent='No synthetic adapter trials. Real execution remains denied.';return;}
  for(const t of records){const card=document.createElement('article');card.className='team-card';paragraph(card,`${t.state} · ${t.scope} · Owner: ${t.envelope.owner} · Event: ${t.sequence}`);paragraph(card,t.reason);paragraph(card,`Deadline: ${t.envelope.deadline} · Pending operation: ${t.pending_operation||'none'} · Real execution admitted: no`);if(t.evidence_digest)inspect(card,t.evidence_digest);if(t.state==='unknown')paragraph(card,'Reconcile the existing handle before retrying. A lost acknowledgment does not establish failure.');panel.append(card);}
}
