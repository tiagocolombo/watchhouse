interface Case { id:string; title:string; description:string; owner:string; status:string; version:number }
interface TimelineEvent { cursor:number; event_id:string; type:string; recorded_at:string; actor:{kind:string;id:string}; payload:{reason:string;evidence_refs?:string[]} }
interface QualificationPlan { kind:string; after_seconds:number; recovery_indicators?:string; collection_task?:string }
interface Disposition { id:string; state:string; occurrence_count:number; recorded_at:string; due_at:string; record:{owner:string;scope:string;uncertainty:string;reason:string;evidence_refs:string[];reconsideration_conditions:string;reopen_after_occurrences:number;plan:QualificationPlan} }
interface Obligation { id:string; owner:string; task:string; due_at:string; state:string; triggered_at:string|null }
interface EvidenceReference { digest:string; provenance:string; recorded_at:string; summary:string }
interface Qualification { dispositions:Disposition[]; obligations:Obligation[]; evidence:EvidenceReference[] }
interface Snapshot { case:Case; events:TimelineEvent[]; cursor:number; results:{summary:string;conclusion:string}[]; qualification:Qualification }
const el = <T extends HTMLElement = HTMLElement>(id:string) => document.getElementById(id) as T;
const input = (id:string) => el<HTMLInputElement>(id).value;
let token = sessionStorage.getItem('watchhouse-token') ?? '';
let selected:Case | undefined;
let qualification:Qualification = {dispositions:[],obligations:[],evidence:[]};
let evidenceCase = "";
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
  el('case-id').textContent=selected.id; el('case-title').textContent=selected.title;
  el('case-status').textContent=selected.status.replaceAll('_',' ');
  el('version').textContent=`Version ${selected.version}`; el('owner').textContent=`Owner: ${selected.owner}`;
  el('case-description').textContent=selected.description; el<HTMLInputElement>('rename-title').value=selected.title;
  el('next').textContent=selected.status==='awaiting_review'?'Next: operator reviews synthetic evidence and decides the next investigation. Case remains open.':selected.status==='running'?'Next: fake investigator returns a bounded result. Operator remains accountable.':'Next: coordinator claims the queued investigation. Operator remains accountable.';
  el('results').replaceChildren();
  for(const result of snapshot.results) { const p=document.createElement('p');p.textContent=result.summary; el('results').append(p); }
  if (!snapshot.results.length) el('results').textContent='No result recorded yet.';
  renderQualification(snapshot);
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
    pending={signature,body:{schema_version:1,idempotency_key:crypto.randomUUID(),actor:'operator',expected_version:expectedVersion,...fields}};
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
    for(const id of ['evidence-form','disposition-form','reconsider-form']) el<HTMLFormElement>(id).reset();
    el<HTMLInputElement>('disposition-owner').value=snapshot.case.owner;planFields();
  }
  const refs=el('evidence-refs');refs.replaceChildren();
  for(const evidence of qualification.evidence) {
    const label=document.createElement('label');const checkbox=document.createElement('input');checkbox.type='checkbox';checkbox.value=evidence.digest;
    checkbox.checked=previous.get(evidence.digest)??true;
    label.append(checkbox,document.createTextNode(`${evidence.provenance==='operator-reported-v1'?'Operator report':'Synthetic result'}: ${evidence.summary} (${evidence.digest.slice(0,19)}…)`));refs.append(label);
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
if(token) void connect().catch(fail);
