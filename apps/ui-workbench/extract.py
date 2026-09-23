"""Generate source-referenced numeric metadata for the native GPUI workbench.

Colors and fonts are read from the live GPUI Theme, never approximated here.
Run: python3 apps/ui-workbench/extract.py [--check]
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).with_name('metrics.json')
FILES = {
    'theme': ROOT / 'crates/ui/src/theme.rs',
    'chrome': ROOT / 'crates/ui/src/surface_chrome.rs',
    'composer': ROOT / 'crates/ui/src/composer.rs',
    'settings': ROOT / 'crates/ui/src/settings/widgets.rs',
    'popover': ROOT / 'crates/ui/src/popover.rs',
    'typography': ROOT / 'crates/ui/src/typography.rs',
    'motion': ROOT / 'crates/ui/src/motion.rs',
}
SOURCES = {key: path.read_text() for key, path in FILES.items()}
METRIC_SPECS = {
    'Theme': ('theme', ['HEADER_HEIGHT', 'TITLEBAR_HEIGHT', 'TITLEBAR_TOP_PAD', 'STATUS_STRIP_HEIGHT', 'TRANSCRIPT_FADE_BAND', 'BUBBLE_RADIUS', 'PANEL_RADIUS', 'CONTROL_RADIUS', 'SPACE_XS', 'SPACE_SM', 'SPACE_MD', 'SPACE_LG', 'TEXT_STACK_GAP']),
    'Chrome': ('chrome', ['CONTROL_SIZE', 'ICON_SIZE', 'CONTROL_GAP', 'EDGE_INSET']),
    'Composer': ('composer', ['TEXTAREA_MIN', 'TEXTAREA_MAX', 'COMPOSER_RADIUS', 'COMPOSER_MAX_WIDTH', 'COMPACT_TOTAL_HEIGHT', 'INPUT_LINE_HEIGHT', 'INPUT_TEXT_SIZE']),
    'Settings': ('settings', ['ROW_TITLE_SIZE', 'ROW_DESCRIPTION_SIZE', 'OPTION_CARD_HEIGHT', 'OPTION_CARD_RADIUS']),
    'Popover': ('popover', ['CARD_RADIUS', 'CARD_INSET', 'MENU_GAP']),
    'Typography': ('typography', ['CODE_FONT_SIZE_DEFAULT', 'TERMINAL_FONT_SIZE_DEFAULT']),
}


def location(key: str, offset: int) -> str:
    return f"{FILES[key].relative_to(ROOT)}:{SOURCES[key].count(chr(10), 0, offset) + 1}"


def extract() -> dict:
    digest = hashlib.sha256()
    for key in sorted(SOURCES):
        digest.update(key.encode())
        digest.update(b'\0')
        digest.update(SOURCES[key].encode())
        digest.update(b'\0')

    metrics = {}
    for group, (key, names) in METRIC_SPECS.items():
        metrics[group] = []
        for name in names:
            match = re.search(r'\b(?:pub(?:\(crate\))?\s+)?const\s+' + name + r':\s*f32\s*=\s*([0-9.]+);', SOURCES[key])
            if not match:
                raise ValueError(f'Missing native metric {key}::{name}')
            metrics[group].append({
                'name': name, 'value': float(match[1]), 'unit': 'px',
                'source': location(key, match.start()),
            })

    motion_source = SOURCES['motion']
    easing = {
        match[1]: list(map(float, match.groups()[1:]))
        for match in re.finditer(r'^pub const (\w+): CubicBezier = CubicBezier::new\(([^,]+),\s*([^,]+),\s*([^,]+),\s*([^)]+)\)', motion_source, re.M)
    }
    motion = []
    for match in re.finditer(r'^pub const (\w+): MotionSpec = MotionSpec::new\((\d+),\s*(\w+)\)', motion_source, re.M):
        motion.append({
            'name': match[1], 'duration': int(match[2]), 'curve': match[3],
            'bezier': easing.get(match[3]), 'source': location('motion', match.start()),
        })
    return {
        'sourceFingerprint': digest.hexdigest()[:12],
        'metrics': metrics,
        'motion': motion,
    }


if __name__ == '__main__':
    data = extract()
    output = json.dumps(data, indent=2) + '\n'
    if '--check' in sys.argv:
        if not OUT.exists() or OUT.read_text() != output:
            raise SystemExit('metrics.json is stale; run extract.py')
        print('Native metric reference is current.')
    else:
        OUT.write_text(output)
        print(f'Extracted {sum(map(len, data["metrics"].values()))} metrics and {len(data["motion"])} motion specs.')
