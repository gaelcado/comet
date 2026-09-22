"""Build, verify and package the standalone gallery and both SVG sizes."""
from pathlib import Path
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parent
subprocess.run([sys.executable, str(ROOT / 'build.py')], check=True)
subprocess.run(['node', str(ROOT / 'verify.cjs')], check=True)
files = [p for p in ROOT.iterdir() if p.suffix in {'.html', '.css', '.js', '.json', '.py', '.cjs', '.md'} and p.name != 'verification.json']
files += [ROOT / 'favicon.svg', *sorted((ROOT / 'svg').glob('*.svg')), *sorted((ROOT / 'svg-small').glob('*.svg'))]
with zipfile.ZipFile(ROOT / 'zeron-icons.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(files):
        entry = zipfile.ZipInfo(str(path.relative_to(ROOT)), (2026, 9, 22, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_DEFLATED
        entry.external_attr = 0o644 << 16
        archive.writestr(entry, path.read_bytes())
print('Created zeron-icons.zip')
