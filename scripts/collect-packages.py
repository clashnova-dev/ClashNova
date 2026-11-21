#!/usr/bin/env python3
"""Collect installable packages and inspect Linux launcher metadata and icons."""
import argparse
import configparser
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def verify_linux(tree, name, version):
    desktops = list(tree.rglob('*.desktop'))
    assert desktops, 'Launcher entry missing'
    entry = configparser.ConfigParser(interpolation=None)
    entry.read(desktops[0])
    desktop = entry['Desktop Entry']
    assert desktop['Name'] == name, desktop['Name']
    assert Path(shlex.split(desktop['Exec'])[0]).name == name, desktop['Exec']
    assert desktop['Icon'] == name, desktop['Icon']
    icons = list(tree.rglob(name + '.png'))
    sources = {hashlib.sha256(p.read_bytes()).hexdigest() for p in (ROOT / 'src-tauri/icons').glob('*.png')}
    assert any(hashlib.sha256(p.read_bytes()).hexdigest() in sources for p in icons), 'Launcher icon mismatch'
    print('Verified Linux launcher name, executable and ClashNova icon:', version)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', required=True)
    args = parser.parse_args()
    config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
    name, version = config['productName'], config['version']
    bundle = ROOT / 'src-tauri/target' / args.target / 'release/bundle'
    output = ROOT / 'release-artifacts'
    output.mkdir(exist_ok=True)
    patterns = ['nsis/*-setup.exe'] if 'windows' in args.target else ['deb/*.deb', 'appimage/*.AppImage'] if 'linux' in args.target else ['dmg/*.dmg']
    records = []
    for pattern in patterns:
        packages = list(bundle.glob(pattern))
        assert packages, 'Package missing: ' + pattern
        for package in packages:
            assert name in package.name and version in package.name, package.name
            if package.suffix == '.deb':
                actual = subprocess.check_output(['dpkg-deb', '-f', package, 'Version'], text=True).strip()
                maintainer = subprocess.check_output(['dpkg-deb', '-f', package, 'Maintainer'], text=True).strip()
                assert actual == version and 'clashnova-dev' in maintainer
                with tempfile.TemporaryDirectory() as tmp:
                    subprocess.run(['dpkg-deb', '-x', package, tmp], check=True)
                    verify_linux(Path(tmp), name, version)
            elif package.suffix == '.AppImage':
                with tempfile.TemporaryDirectory() as tmp:
                    subprocess.run([str(package), '--appimage-extract'], cwd=tmp, check=True, stdout=subprocess.DEVNULL)
                    verify_linux(Path(tmp) / 'squashfs-root', name, version)
            shutil.copy2(package, output / package.name)
            records.append({'file': package.name, 'sha256': hashlib.sha256(package.read_bytes()).hexdigest()})
    (output / 'package-metadata.json').write_text(json.dumps({'name': name, 'version': version, 'publisher': 'clashnova-dev', 'target': args.target, 'files': records}, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f"{item['sha256']}  {item['file']}\n" for item in records))
    print(json.dumps(records))


if __name__ == '__main__':
    main()
