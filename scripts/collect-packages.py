#!/usr/bin/env python3
"""Collect installable packages and inspect Linux launcher metadata and icons."""
import argparse
import configparser
import hashlib
import json
import os
import plistlib
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
    if 'apple' in args.target:
        apps = list((bundle / 'macos').glob('*.app'))
        assert len(apps) == 1, 'Expected one macOS application'
        contents = apps[0] / 'Contents'
        info = plistlib.loads((contents / 'Info.plist').read_bytes())
        assert info['CFBundleName'] == name and info['CFBundleShortVersionString'] == version
        assert info['CFBundleExecutable'] == name
        icon_name = info['CFBundleIconFile']
        if not icon_name.endswith('.icns'):
            icon_name += '.icns'
        assert (contents / 'Resources' / icon_name).read_bytes() == (ROOT / 'src-tauri/icons/icon.icns').read_bytes()
        print('Verified macOS application name, version, executable and ClashNova icon')
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

    updater = None
    if os.environ.get('TAURI_SIGNING_PRIVATE_KEY'):
        if 'apple' in args.target:
            candidates = list((bundle / 'macos').glob('*.app.tar.gz'))
            platform = 'darwin-' + args.target.split('-')[0]
        elif 'windows' in args.target:
            candidates = list((bundle / 'nsis').glob('*-setup.exe'))
            platform = 'windows-' + args.target.split('-')[0]
        else:
            candidates = list((bundle / 'appimage').glob('*.AppImage'))
            platform = 'linux-' + args.target.split('-')[0]
        assert len(candidates) == 1, 'Expected exactly one updater artifact'
        artifact = candidates[0]
        signature = Path(str(artifact) + '.sig')
        assert signature.is_file() and signature.read_text().strip(), 'Updater signature missing'
        filename = (name + '_' + version + '_' + args.target.split('-')[0] + '.app.tar.gz') if 'apple' in args.target else artifact.name
        for source, destination in [(artifact, filename), (signature, filename + '.sig')]:
            shutil.copy2(source, output / destination)
            if not any(item['file'] == destination for item in records):
                records.append({'file': destination, 'sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
        updater = {'platform': platform, 'file': filename, 'signature': signature.read_text().strip()}

    (output / 'package-metadata.json').write_text(json.dumps({'name': name, 'version': version, 'publisher': 'clashnova-dev', 'target': args.target, 'files': records, 'updater': updater}, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f"{item['sha256']}  {item['file']}\n" for item in records))
    print(json.dumps(records))


if __name__ == '__main__':
    main()
