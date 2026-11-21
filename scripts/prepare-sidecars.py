#!/usr/bin/env python3
"""Build the desktop sidecars from source for one target (needs Python 3, Go and Rust).

No prebuilt executable is downloaded. The sing-box core is compiled from core/, where
core/go.mod pins the sing-box version and core/go.sum verifies it; the subscription
converter is compiled from a pinned, checksummed module. Go cross-compiles both, so
every desktop target comes from the same source.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from tools import go_cmd, rustc_cmd  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORE = ROOT / 'core'
BINARIES = ROOT / 'src-tauri' / 'binaries'
SING_BOX = 'github.com/sagernet/sing-box'
# What the client uses: the TUN stack, QUIC protocols, DHCP DNS, WireGuard, uTLS and the
# local traffic API, plus the two tags sing-box needs to link. Left out on purpose: naive
# (needs a prebuilt Chromium network library for each platform), Tailscale, ACME,
# Cloudflare Tunnel, OpenVPN, OpenConnect, USB/IP and the CCM/OCM services.
CORE_TAGS = 'with_gvisor,with_quic,with_dhcp,with_wireguard,with_utls,with_clash_api,badlinkname,tfogo_checklinkname0'
CONVERTER = ('github.com/rainbend/sing-box-subscribe-cli', 'v1.0.4', 'h1:9eCpl5YKdb81WgDENQCdqikSbWtm1JS3Pj+/e0TMGVc=')
TARGETS = {
    'x86_64-pc-windows-msvc': ('windows', 'amd64'),
    'aarch64-apple-darwin': ('darwin', 'arm64'),
    'x86_64-apple-darwin': ('darwin', 'amd64'),
    'x86_64-unknown-linux-gnu': ('linux', 'amd64'),
    'aarch64-unknown-linux-gnu': ('linux', 'arm64'),
}


def module(spec):
    """Download a module into the Go module cache (a no-op once cached) and describe it."""
    return json.loads(subprocess.check_output([*go_cmd(), 'mod', 'download', '-json', spec], cwd=CORE, text=True))


def main():
    go_cmd()  # fail fast with a clear message if Go is missing
    host = subprocess.check_output([*rustc_cmd(), '--print', 'host-tuple'], text=True).strip()
    parser = argparse.ArgumentParser(description=__doc__)
    # Tauri sets TAURI_ENV_TARGET_TRIPLE for its build hooks.
    parser.add_argument('--target', default=os.environ.get('TAURI_ENV_TARGET_TRIPLE', host), choices=TARGETS)
    parser.add_argument('--test', action='store_true', help="run the core's Go tests with the shipped build settings instead of building")
    arguments = parser.parse_args()
    target = arguments.target
    system, arch = TARGETS[target]
    suffix = '.exe' if system == 'windows' else ''
    env = {**os.environ, 'GOOS': system, 'GOARCH': arch, 'CGO_ENABLED': '0', 'GOTOOLCHAIN': 'go1.27.4+auto'}
    BINARIES.mkdir(exist_ok=True)

    sing_box = module(SING_BOX)
    source = pathlib.Path(sing_box['Dir'])
    # Link exactly as upstream does, with the version the module is pinned to.
    ldflags = f"-X {SING_BOX}/constant.Version={sing_box['Version'].lstrip('v')} {(source / 'release' / 'LDFLAGS').read_text().strip()} -s -w -buildid="
    if arguments.test:
        settings = ['-tags', CORE_TAGS, '-ldflags', ldflags]
        subprocess.run([*go_cmd(), 'vet', '-tags', CORE_TAGS, './...'], cwd=CORE, env={**os.environ, 'CGO_ENABLED': '0'}, check=True)
        subprocess.run([*go_cmd(), 'test', *settings, './...'], cwd=CORE, env={**os.environ, 'CGO_ENABLED': '0'}, check=True)
        return
    core = BINARIES / f'airport-core-{target}{suffix}'
    subprocess.run([*go_cmd(), 'build', '-trimpath', '-tags', CORE_TAGS, '-ldflags', ldflags, '-o', str(core), '.'], cwd=CORE, env=env, check=True)
    shutil.copyfile(source / 'LICENSE', BINARIES / 'sing-box-LICENSE')

    path, version, checksum = CONVERTER
    converter_module = module(f'{path}@{version}')
    if converter_module['Sum'] != checksum:
        raise RuntimeError('subscription converter checksum mismatch')
    source = pathlib.Path(converter_module['Dir'])
    converter = BINARIES / f'sing-box-sub-{target}{suffix}'
    subprocess.run([*go_cmd(), 'build', '-trimpath', '-ldflags', f'-s -w -X main.version={version}', '-o', str(converter), './cmd/sing-box-subscribe-cli'], cwd=source, env=env, check=True)
    shutil.copyfile(source / 'LICENSE', BINARIES / 'sing-box-sub-LICENSE')

    if target == host:
        subprocess.run([str(core), 'version'], check=True)
        subprocess.run([str(converter), 'version'], check=True)
    print('Prepared sidecars:', target)


if __name__ == '__main__':
    main()
