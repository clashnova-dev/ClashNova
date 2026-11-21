#!/usr/bin/env python3
"""Build the privileged helper from this repository; never download an executable."""
import os, pathlib, shutil, subprocess
root = pathlib.Path(__file__).resolve().parent.parent
host = subprocess.check_output(['rustc', '--print', 'host-tuple'], text=True).strip()
target = os.environ.get('TAURI_ENV_TARGET_TRIPLE', host)
cmd = ['cargo', 'build', '--locked', '--release', '--manifest-path', str(root/'tun-service/Cargo.toml'), '--target', target]
# Optional local registry override; normal release CI uses crates.io.
if os.environ.get('AIRPORT_CARGO_MIRROR') == 'rsproxy':
    cmd += ['--config', 'source.crates-io.replace-with="rsproxy-sparse"', '--config', 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"']
subprocess.run(cmd, check=True)
suffix = '.exe' if 'windows' in target else ''
source = root/'tun-service/target'/target/'release'/('airport-tun-service'+suffix)
destination = root/'src-tauri/binaries'/('airport-tun-service-'+target+suffix)
destination.parent.mkdir(exist_ok=True)
shutil.copy2(source, destination)
print('Prepared TUN helper:', target)
