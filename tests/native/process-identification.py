import subprocess,socket,tempfile,json,time,pathlib,urllib.request,threading,os
# Opt-in real-core regression; uses only loopback and never changes system proxy/TUN.
import platform
root = pathlib.Path(__file__).resolve().parents[2]
system = platform.system()
arch = 'aarch64' if platform.machine().lower() in ('arm64', 'aarch64') else 'x86_64'
target = arch + {'Darwin': '-apple-darwin', 'Linux': '-unknown-linux-gnu', 'Windows': '-pc-windows-msvc'}[system]
binary = os.environ.get('AIRPORT_TEST_CORE', str(root / 'src-tauri' / 'binaries' / ('airport-core-' + target + ('.exe' if system == 'Windows' else ''))))
def port():
 s=socket.socket();s.bind(('127.0.0.1',0));p=s.getsockname()[1];s.close();return p
def run(enabled):
 proxy,api=port(),port();secret='local-test-only';target=socket.socket();target.bind(('127.0.0.1',0));target.listen();target.settimeout(5)
 with tempfile.TemporaryDirectory() as d:
  c={'log':{'disabled':True},'inbounds':[{'type':'mixed','listen':'127.0.0.1','listen_port':proxy}], 'outbounds':[{'type':'direct','tag':'direct'}], 'route':{'final':'direct','find_process':enabled},'experimental':{'clash_api':{'external_controller':f'127.0.0.1:{api}','secret':secret}}}
  path=pathlib.Path(d)/'config.json';path.write_text(json.dumps(c));path.chmod(0o600)
  process=subprocess.Popen([binary,'run','-c',str(path)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
  peer=None;client=None
  try:
   for i in range(100):
    try:client=socket.create_connection(('127.0.0.1',proxy),.1);break
    except OSError:time.sleep(.03)
   if client is None:raise RuntimeError(process.stderr.read(2048).decode())
   client.settimeout(4)
   client.sendall(f'CONNECT 127.0.0.1:{target.getsockname()[1]} HTTP/1.1\r\nHost: localhost\r\n\r\n'.encode());assert b'200' in client.recv(4096)
   peer,_=target.accept()
   request=urllib.request.Request(f'http://127.0.0.1:{api}/connections',headers={'Authorization':'Bearer '+secret})
   data=json.load(urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request,timeout=3))
   metadata=data['connections'][0]['metadata'];print(json.dumps({'find_process':enabled,'processIdentified':bool(metadata.get('processPath'))},ensure_ascii=False))
   if enabled:assert metadata.get('processPath'), 'process path still missing'
  finally:
   if client:client.close()
   if peer:peer.close()
   target.close();process.terminate();process.wait(timeout=5)
run(False);run(True)
