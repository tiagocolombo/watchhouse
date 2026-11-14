interface Case { id:string; title:string; description:string; owner:string; status:string; version:number }
interface TimelineEvent { cursor:number; event_id:string; type:string; recorded_at:string; actor:{kind:string;id:string}; payload:{reason:string;evidence_refs?:string[]} }
interface Snapshot { case:Case; events:TimelineEvent[]; cursor:number; results:{summary:string;conclusion:string}[] }
const el = <T extends HTMLElement = HTMLElement>(id:string) => document.getElementById(id) as T;
const input = (id:string) => el<HTMLInputElement>(id).value;
let token = sessionStorage.getItem('watchhouse-token') ?? '';
let selected:Case | undefined;
let generation = 0;
let active:AbortController | undefined;
let createKey = crypto.randomUUID();
let createPayload = '';
const seen = new Set<string>();
function fail(error:unknown) { el('error').textContent = error instanceof Error ? error.message : String(error); }
async function api<T>(path:string,body?:unknown):Promise<T> {
  const response = await fetch(`/api/v1${path}`,{method:body?'POST':'GET',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});
  if (!response.ok) { let message = await response.text(); try { message = JSON.parse(message).message ?? message; } catch { /* non-JSON protocol error */ } throw new Error(message); }
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
if(token) void connect().catch(fail);
