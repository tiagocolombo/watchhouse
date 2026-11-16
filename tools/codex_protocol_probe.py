import http.server,threading,subprocess,tempfile,os,json,pathlib,sys
# Explicit no-account protocol drill against a loopback synthetic SSE endpoint.
runtime=pathlib.Path(sys.argv[1]).resolve() if len(sys.argv)>1 else pathlib.Path("/opt/codex/bin/codex")
seen=[]
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def do_GET(self):
  self.send_response(404);self.end_headers()
 def do_POST(self):
  assert not self.headers.get('authorization'), 'fixture must not receive credentials'
  body=self.rfile.read(int(self.headers['content-length']))
  try: value=json.loads(body)
  except Exception as e: value={'decode_error':str(e),'content_encoding':self.headers.get('content-encoding'),'size':len(body)}
  seen.append({'path':self.path,'request':value})
  item={'id':'msg_fixture','type':'message','role':'assistant','status':'completed','content':[{'type':'output_text','text':'{"changes":[],"summary":"fixture only"}'}]}
  events=[{'type':'response.created','response':{'id':'resp_fixture','model':'fixture'}},{'type':'response.output_item.added','output_index':0,'item':item},{'type':'response.output_item.done','output_index':0,'item':item},{'type':'response.completed','response':{'id':'resp_fixture','model':'fixture','status':'completed','output':[item],'usage':{'input_tokens':10,'output_tokens':10,'total_tokens':20}}}]
  data=''.join('event: '+v['type']+'\ndata: '+json.dumps(v)+'\n\n' for v in events).encode();self.send_response(200);self.send_header('content-type','text/event-stream');self.send_header('content-length',str(len(data)));self.end_headers();self.wfile.write(data)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='watchhouse-codex-protocol-',dir=str(pathlib.Path.cwd())) as directory:
 root=pathlib.Path(directory);(root/'home').mkdir();(root/'work').mkdir();schema=root/'result.schema.json';schema.write_text(json.dumps({'type':'object','properties':{'changes':{'type':'array','items':{'type':'string'}},'summary':{'type':'string'}},'required':['changes','summary'],'additionalProperties':False}))
 args=[str(runtime),'exec','--json','--ephemeral','--ignore-user-config','--ignore-rules','--sandbox','read-only','--skip-git-repo-check','--color','never','--output-schema',str(schema),'-c','model_provider="fixture"','-c','model="fixture"','-c','model_providers.fixture.name="Fixture"','-c',f'model_providers.fixture.base_url="http://127.0.0.1:{server.server_port}/v1"','-c','model_providers.fixture.requires_openai_auth=false','-c','model_providers.fixture.wire_api="responses"','-c','model_providers.fixture.request_max_retries=0','-c','model_providers.fixture.stream_max_retries=0','-c','web_search="disabled"']
 for feature in ['collaboration_modes','shell_tool','unified_exec','hooks','apps','plugins','remote_plugin','browser_use','browser_use_external','computer_use','view_image','image_generation','in_app_browser','code_mode','code_mode_host','multi_agent','multi_agent_v2','memories','skill_search','skill_mcp_dependency_install','plugin_discovery_prefetch','goals','sleep_tool','tool_suggest','workspace_dependencies','in_app_chat','enable_request_compression','auth_elicitation']:
  args+=['--disable',feature]
 args+=['--enable','skip_host_skill_discovery','-']
 try:
  out=subprocess.run(args,input='Return an empty changes array and summary "fixture only".',text=True,capture_output=True,timeout=25,cwd=root/'work',env={'PATH':os.environ['PATH'],'HOME':str(root/'home'),'CODEX_HOME':str(root/'home')})
  print('exit',out.returncode,'stdout',out.stdout[:10000],'stderr',out.stderr[:3000])
 except subprocess.TimeoutExpired as e:raise RuntimeError('native runtime timed out; no capability admission') from e
 print('requests',len(seen))
 for s in seen:print(json.dumps({'path':s['path'],'keys':list(s['request']),'tools':s['request'].get('tools'),'other':{k:v for k,v in s['request'].items() if k in ['decode_error','content_encoding','size']}}))
 assert out.returncode == 0, 'native runtime protocol failed; no capability admission'
 assert len(seen) == 1 and seen[0]['path']=='/v1/responses', 'unexpected model endpoint/retry'
 assert not seen[0]['request'].get('tools'), 'tool-free profile still advertises tools'
 events=[json.loads(line) for line in out.stdout.splitlines() if line.strip()]
 assert any(e.get('type')=='turn.completed' for e in events), 'missing completed turn'
 assert any(e.get('type')=='item.completed' and e.get('item',{}).get('type')=='agent_message' for e in events), 'missing final structured message'
 print('Native tool-free Codex protocol fixture passed; no account/model request was used')
server.shutdown()
