#!/usr/bin/env python3
"""Install a pinned, explicit voice runtime into a macOS bundle (no CLI lookup)."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

BUILD = 'a956835d020762cb2b570053af06f643a11c0ecc'
VERSION = '0.160.0'
TARGET = 'aarch64-apple-darwin'


def install(package, destination):
    # The helper only initializes from a `codex-resources/voice` directory; it
    # exits with code 23 on initializeRuntime anywhere else.
    if (destination.parent.name, destination.name) != ('codex-resources', 'voice'):
        raise ValueError('destination must be <bundle>/Contents/Resources/codex-resources/voice')
    source = package / 'codex-resources/voice'
    manifest = json.loads((source / 'manifest.json').read_text())
    if (manifest['buildCommit'], manifest['appVersion'], manifest['voiceTarget']) != (BUILD, VERSION, TARGET):
        raise ValueError('unsupported voice package; expected pinned 0.160.0 macOS ARM64')
    for relative, digest in manifest['sha256'].items():
        name = Path(relative)
        if name.is_absolute() or '..' in name.parts:
            raise ValueError('invalid manifest path')
        path = package / name
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError('voice package checksum mismatch')
    # Reject untracked runtime files and symlinks before signing/copying code.
    for path in source.rglob('*'):
        if path.is_symlink():
            raise ValueError('voice runtime symlinks are unsupported')
        if path.is_file() and path.name != 'manifest.json' and str(path.relative_to(package)) not in manifest['sha256']:
            raise ValueError('untracked runtime resource')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temporary:
        stage = Path(temporary) / 'voice'
        shutil.copytree(source, stage)
        shutil.copyfile(Path(__file__).resolve().parents[1] / "dist/voice/Codex-LICENSE.txt", stage / "licenses/Codex-LICENSE.txt")
        # Ship the runtime byte-for-byte: upstream already signs the helper and
        # every library with OpenAI's Developer ID, hardened runtime and a
        # secure timestamp, so re-signing would only downgrade them.
        runtime = json.loads((stage / 'runtime.json').read_text())
        for library in runtime['libraries']:
            if hashlib.sha256((stage / library['path']).read_bytes()).hexdigest() != library['sha256']:
                raise ValueError(f"runtime library does not match runtime.json: {library['path']}")
        for path in sorted(stage.rglob('*')):
            if path.is_file() and (path.suffix == '.dylib' or path.name == 'codex-voice-host'):
                subprocess.run(['codesign', '--verify', '--strict', str(path)], check=True)
        hashes = {str(p.relative_to(stage)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in sorted(stage.rglob('*')) if p.is_file()}
        (stage / 'zeron-runtime.json').write_text(json.dumps({
            'protocol': 1, 'buildCommit': BUILD, 'sourceVersion': VERSION,
            'target': TARGET, 'sha256': hashes}, indent=2) + '\n')
        if destination.exists():
            shutil.rmtree(destination)
        shutil.move(str(stage), destination)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    install(args.package.resolve(), args.destination.resolve())
