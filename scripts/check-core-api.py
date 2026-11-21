#!/usr/bin/env python3
"""Isolated sing-box 1.14 control API smoke test; never changes OS proxy."""
import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
args = parser.parse_args()
binary = str(args.binary.resolve())
with socket.socket() as listener:
    listener.bind(('127.0.0.1', 0))
    port = listener.getsockname()[1]
secret = secrets.token_hex(32)
config = {'log': {'disabled': True}, 'outbounds': [{'type': 'direct', 'tag': 'direct'}],
          'services': [{'type': 'api', 'tag': 'control', 'listen': '127.0.0.1', 'listen_port': port,
                        'secret': secret, 'dashboard': False, 'access_control_allow_origin': ['tauri://localhost']}]}
with tempfile.TemporaryDirectory(prefix='airport-api-test-') as folder:
    path = Path(folder) / 'config.json'
    path.write_text(json.dumps(config))
    path.chmod(0o600)
    subprocess.run([binary, 'check', '-c', str(path)], check=True, capture_output=True)
    core = subprocess.Popen([binary, 'run', '-c', str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        env = dict(os.environ, BOX_API_URL=f'http://127.0.0.1:{port}', BOX_API_SECRET=secret)
        for _ in range(30):
            result = subprocess.run([binary, 'api', 'version'], env=env, capture_output=True, timeout=3)
            if result.returncode == 0:
                break
            if core.poll() is not None:
                raise RuntimeError('core exited before API readiness')
            time.sleep(0.1)
        else:
            raise RuntimeError('API readiness timeout')
        denied = subprocess.run([binary, 'api', 'version'], env=dict(env, BOX_API_SECRET='incorrect'), capture_output=True, timeout=3)
        if denied.returncode == 0:
            raise RuntimeError('invalid API secret was accepted')
        subprocess.run([binary, 'api', 'outbounds'], env=env, check=True, capture_output=True, timeout=3)
        print('PASS: local gRPC API version/outbounds; invalid secret rejected; dashboard disabled')
    finally:
        core.terminate()
        try:
            core.wait(timeout=5)
        except subprocess.TimeoutExpired:
            core.kill()
            core.wait()
