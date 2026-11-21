#!/usr/bin/env python3
"""Validate application identity and produce the non-secret CI build overlay."""
import argparse
import json
import os
import re
from pathlib import Path
import struct
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ci-config', action='store_true')
    parser.add_argument('--platform', default='windows-linux', choices=['windows-linux', 'windows', 'linux', 'all'])
    args = parser.parse_args()
    config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
    package = json.loads((ROOT / 'package.json').read_text())
    cargo = tomllib.loads((ROOT / 'src-tauri/Cargo.toml').read_text())['package']
    assert config['productName'] == config['mainBinaryName'] == 'ClashNova'
    assert config['version'] == package['version'] == cargo['version'], 'Application versions differ'
    assert config['bundle']['publisher'] == 'clashnova-dev'
    assert cargo['authors'] == ['clashnova-dev']
    rust_source = ROOT / 'src-tauri/src/connection.rs'
    for included in re.findall(r'include_str!\("([^"]+)"\)', rust_source.read_text()):
        assert (rust_source.parent / included).is_file(), 'Missing Rust contract include: ' + included
    for name, size in [('32x32.png', 32), ('128x128.png', 128), ('128x128@2x.png', 256)]:
        image = (ROOT / 'src-tauri/icons' / name).read_bytes()
        assert image[:8] == b'\x89PNG\r\n\x1a\n'
        assert struct.unpack('>II', image[16:24]) == (size, size), name
    icon = (ROOT / 'src-tauri/icons/icon.ico').read_bytes()
    reserved, kind, count = struct.unpack('<HHH', icon[:6])
    assert reserved == 0 and kind == 1 and count > 0, 'Invalid Windows icon'
    matrix = [
        {'os': 'windows-latest', 'target': 'x86_64-pc-windows-msvc', 'bundles': 'nsis', 'label': 'Windows-x86_64'},
        {'os': 'ubuntu-22.04', 'target': 'x86_64-unknown-linux-gnu', 'bundles': 'deb,appimage', 'label': 'Linux-x86_64'},
    ]
    if args.platform in ['windows', 'linux']:
        matrix = [row for row in matrix if row['label'].lower().startswith(args.platform)]
    if args.platform == 'all':
        matrix += [
            {'os': 'macos-15', 'target': 'aarch64-apple-darwin', 'bundles': 'app,dmg', 'label': 'macOS-arm64'},
            {'os': 'macos-15-intel', 'target': 'x86_64-apple-darwin', 'bundles': 'app,dmg', 'label': 'macOS-x86_64'},
        ]
    outputs = {'name': config['productName'], 'version': config['version'], 'matrix': json.dumps({'include': matrix})}
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as out:
            for key, value in outputs.items():
                out.write(f'{key}={value}\n')
    if args.ci_config:
        directory = ROOT / '.release'
        directory.mkdir(exist_ok=True)
        signed = bool(os.environ.get('TAURI_SIGNING_PRIVATE_KEY'))
        (directory / 'tauri-ci.json').write_text(json.dumps({'bundle': {'createUpdaterArtifacts': signed}}) + '\n')
        print('Updater signing:', 'enabled' if signed else 'not configured; building installers only')
    print(json.dumps(outputs))


if __name__ == '__main__':
    main()
