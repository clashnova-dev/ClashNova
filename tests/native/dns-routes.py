"""Opt-in real-core DNS regression using configs exported by tun::tests.
Usage: python3 tests/native/dns-routes.py PRIVATE_EXPORT_DIRECTORY
No system proxy or virtual interface is changed. Never prints configuration credentials.
"""
import concurrent.futures, json, os, pathlib, socket, struct, subprocess, sys, time
ROOT = pathlib.Path(__file__).resolve().parents[2]
BINARY = os.environ.get('AIRPORT_TEST_CORE', str(ROOT / 'src-tauri/binaries/airport-core-aarch64-apple-darwin'))
DIRECTORY = pathlib.Path(sys.argv[1])
def reserve():
 with socket.socket() as s:
  s.bind(('127.0.0.1',0));return s.getsockname()[1]
def query(port, name, kind):
 ident=1234
 data=struct.pack('!6H',ident,0x100,1,0,0,0)+b''.join(bytes([len(x)])+x.encode() for x in name.split('.'))+b'\0'+struct.pack('!2H',kind,1)
 with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as s:
  s.settimeout(7);start=time.monotonic();s.sendto(data,('127.0.0.1',port));result=s.recv(4096)
 flags,answers=struct.unpack('!H',result[2:4])[0],struct.unpack('!H',result[6:8])[0]
 return {'rcode':flags&15,'answers':answers,'ms':round((time.monotonic()-start)*1000)},result
results=[]
for mode in ['system','fakeip','remote','unavailable-local']:
 c=json.loads((DIRECTORY/(('fakeip' if mode=='unavailable-local' else mode)+'.json')).read_text())
 proxy,dns=reserve(),reserve();c.pop('experimental',None)
 c['log']={'level':'error','timestamp':False}
 c['inbounds']=[{'type':'mixed','tag':'mixed-in','listen':'127.0.0.1','listen_port':proxy}]
 if mode!='system':c['inbounds'].append({'type':'direct','tag':'tun-in','listen':'127.0.0.1','listen_port':dns})
 if mode=='unavailable-local':
  for server in c['dns']['servers']:
   if server['tag']=='airport-local':server['server']='127.0.0.1';server['server_port']=9
 path=DIRECTORY/(mode+'-runtime.json');path.write_text(json.dumps(c));path.chmod(0o600)
 with open(DIRECTORY/(mode+'.log'),'w') as log:
  core=subprocess.Popen([BINARY,'run','-c',str(path)],stdout=log,stderr=log)
  try:
   for _ in range(100):
    try:
     with socket.create_connection(('127.0.0.1',proxy),.1):pass
     break
    except OSError:
     if core.poll() is not None:raise RuntimeError(mode+' core exited')
     time.sleep(.03)
   if mode!='system':
    for kind in [1,28,65,64]:
     result,raw=query(dns,'www.cloudflare.com',kind);results.append({'mode':mode,'query':kind,**result});assert result['rcode']==0,results[-1]
     if mode in ['fakeip','unavailable-local']:
      assert result['answers']==(1 if kind==1 else 0),results[-1]
      if kind==1:
       assert raw[-4:-2]==bytes([198,18]) or raw[-4:-2]==bytes([198,19]);assert struct.pack('!I',30) in raw
   if mode=='unavailable-local':continue
   jobs=[(target,i) for target in ['https://www.cloudflare.com/cdn-cgi/trace','https://www.youtube.com/generate_204','https://www.baidu.com'] for i in range(3)]
   def request(job):
    target,i=job
    r=subprocess.run(['curl','--noproxy','','--proxy',f'http://127.0.0.1:{proxy}','--connect-timeout','4','--max-time','10','-sS','-o','/dev/null','-w','%{http_code} %{time_total}',target],capture_output=True,text=True)
    return {'mode':mode,'target':target.split('/')[2],'sample':i,'ok':r.returncode==0,'result':r.stdout,'error':r.stderr.strip()[:120]}
   with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    for result in pool.map(request,jobs):results.append(result)
  finally:core.terminate();core.wait(timeout=5);path.unlink()
for result in results:print(json.dumps(result),flush=True)
assert all(result.get('ok',True) for result in results), 'Some real-node requests failed; retain this result.'
