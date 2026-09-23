"""Extract the desktop design contract into a browser-readable reference.

Run from any directory: python3 apps/ui-companion/extract.py [--check].
Only source-backed roles are emitted; unsupported expressions fail loudly.
"""
from __future__ import annotations
import json
import hashlib
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).with_name('styleguide.json')
THEME = ROOT / 'crates/ui/src/theme.rs'
MOTION = ROOT / 'crates/ui/src/motion.rs'
FILES = {
    'theme': THEME,
    'chrome': ROOT / 'crates/ui/src/surface_chrome.rs',
    'composer': ROOT / 'crates/ui/src/composer.rs',
    'settings': ROOT / 'crates/ui/src/settings/widgets.rs',
    'popover': ROOT / 'crates/ui/src/popover.rs',
    'typography': ROOT / 'crates/ui/src/typography.rs',
    'motion': MOTION,
}
SOURCE = {key: path.read_text() for key, path in FILES.items()}
COLOR_ROLES = {
    'Surfaces': ['bg', 'surface', 'surface_raised', 'surface_card', 'surface_dialog', 'surface_overlay', 'input_bg', 'band'],
    'Text': ['text', 'text_muted', 'text_faint', 'text_dim', 'solid', 'on_solid'],
    'Structure': ['border', 'border_strong', 'element_hover', 'element_active', 'surface_raised_hover'],
    'Accent': ['accent', 'accent_strong', 'accent_wash', 'on_accent', 'selection', 'caret'],
    'Feedback': ['danger', 'danger_muted', 'danger_strong', 'warning', 'warning_muted', 'success', 'success_muted', 'busy'],
    'Code and diff': ['code_text', 'code_wash', 'diff_add', 'diff_del', 'diff_hunk_bg'],
}

def line_of(key: str, offset: int) -> str:
    return f"{FILES[key].relative_to(ROOT)}:{SOURCE[key].count(chr(10), 0, offset) + 1}"

def gamma(x: float) -> float:
    x = max(0, min(1, x))
    return 12.92 * x if x <= .0031308 else 1.055 * x ** (1 / 2.4) - .055

def oklch(l: float, c: float, h: float) -> str:
    a, b = c * math.cos(math.radians(h)), c * math.sin(math.radians(h))
    x = (l + .39633778*a + .21580376*b) ** 3
    y = (l - .105561346*a - .06385417*b) ** 3
    z = (l - .08948418*a - 1.2914855*b) ** 3
    rgb = [gamma(4.0767417*x - 3.3077116*y + .23096993*z),
           gamma(-1.268438*x + 2.6097574*y - .3413194*z),
           gamma(-.0041960863*x - .7034186*y + 1.7076147*z)]
    return '#' + ''.join(f'{round(v*255):02x}' for v in rgb)

def render(expr: str, accent: dict, appearance: str) -> str:
    expr = expr.strip()
    if expr.startswith('band_for('):
        band = re.search(r'Appearance::' + appearance.title() + r'\s*=>\s*(hsla\([^)]*\))', SOURCE['theme'][SOURCE['theme'].index('fn band_for('):])
        if not band:
            raise ValueError('Missing band color')
        return render(band[1], accent, appearance)
    if expr.startswith('accent.'):
        attr = expr.split('.', 1)[1]
        value = {
            'primary': accent['primary'], 'strong': accent['strong'],
            'activity': accent['primary'], 'caret': accent['primary'],
            'code_text': accent['primary'],
            'wash': f"color-mix(in srgb, {accent['strong'] if appearance == 'dark' else accent['primary']} {45 if appearance == 'dark' else 10}%, transparent)",
            'selection': f"color-mix(in srgb, {accent['primary']} {35 if appearance == 'dark' else 24}%, transparent)",
            'code_wash': f"color-mix(in srgb, {accent['primary']} {12 if appearance == 'dark' else 10}%, transparent)",
        }
        return value[attr]
    m = re.fullmatch(r'(grey|neutral|oklch|hsla)\(([^)]+)\)', expr)
    if not m:
        raise ValueError(f'Unsupported theme expression: {expr}')
    kind, args = m[1], [v.strip() for v in m[2].split(',')]
    if kind == 'grey':
        v = int(args[0], 0)
        return f'#{v:02x}{v:02x}{v:02x}'
    if kind == 'neutral':
        return oklch(float(args[0]), 0, 0)
    if kind == 'oklch':
        return oklch(*map(float, args))
    h, s, l, alpha = map(float, args)
    return f'hsl({h * 360:g}deg {s * 100:g}% {l * 100:g}% / {alpha:g})'

def extract() -> dict:
    fingerprint = hashlib.sha256()
    for key in sorted(SOURCE):
        fingerprint.update(key.encode())
        fingerprint.update(b'\0')
        fingerprint.update(SOURCE[key].encode())
        fingerprint.update(b'\0')
    src = SOURCE['theme']
    blocks = {appearance: src.split(f'pub fn {appearance}_with_accent(', 1)[1].split('\n    /// Build the ', 1)[0].split('\n    /// Build theme', 1)[0]
              for appearance in ('dark', 'light')}
    # Bound the dark block before its paired light constructor.
    blocks['dark'] = blocks['dark'].split('pub fn light()', 1)[0]
    accents = {}
    for name in ('Zeron', 'Orange', 'Amber', 'Green', 'Cyan', 'Blue', 'Pink'):
        accents[name.lower()] = {}
        for appearance in ('Dark', 'Light'):
            pattern = (r'\(Self::' + name + r', Appearance::' + appearance +
                       r'\)\s*=>\s*(?:\{\s*)?\(oklch\(([^)]+)\),\s*oklch\(([^)]+)\)\)')
            match = re.search(pattern, src)
            if not match:
                raise ValueError(f'Missing {name}/{appearance} accent in theme.rs')
            accents[name.lower()][appearance.lower()] = {
                'primary': oklch(*map(float, match[1].split(','))),
                'strong': oklch(*map(float, match[2].split(','))),
                'source': line_of('theme', match.start()),
            }
    colors = {}
    for appearance, block in blocks.items():
        start = src.index(block)
        colors[appearance] = {}
        for category, roles in COLOR_ROLES.items():
            for role in roles:
                match = re.search(r'^\s*' + role + r':\s*(.+?),\s*(?://.*)?$', block, re.M)
                if not match:
                    if role == 'band':
                        colors[appearance][role] = {'value': 'color-mix(in srgb, var(--text) 7%, transparent)' if appearance == 'light' else 'color-mix(in srgb, var(--text) 16%, transparent)', 'expression': f'band_for(Appearance::{appearance.title()})', 'source': line_of('theme', src.index('pub fn ' + appearance + '_with_accent'))}
                        continue
                    raise ValueError(f'Missing theme role {appearance}.{role}')
                expr = match[1].strip()
                colors[appearance][role] = {'value': render(expr, accents['zeron'][appearance], appearance),
                                            'expression': expr, 'source': line_of('theme', start + match.start())}
    numbers = {}
    metric_specs = {
        'Theme': ('theme', ['HEADER_HEIGHT','TITLEBAR_HEIGHT','TITLEBAR_TOP_PAD','STATUS_STRIP_HEIGHT','TRANSCRIPT_FADE_BAND','BUBBLE_RADIUS','PANEL_RADIUS','CONTROL_RADIUS','SPACE_XS','SPACE_SM','SPACE_MD','SPACE_LG','TEXT_STACK_GAP']),
        'Chrome': ('chrome', ['CONTROL_SIZE','ICON_SIZE','CONTROL_GAP','EDGE_INSET']),
        'Composer': ('composer', ['TEXTAREA_MIN','TEXTAREA_MAX','COMPOSER_RADIUS','COMPOSER_MAX_WIDTH','COMPACT_TOTAL_HEIGHT','INPUT_LINE_HEIGHT','INPUT_TEXT_SIZE']),
        'Settings': ('settings', ['ROW_TITLE_SIZE','ROW_DESCRIPTION_SIZE','OPTION_CARD_HEIGHT','OPTION_CARD_RADIUS']),
        'Popover': ('popover', ['CARD_RADIUS','CARD_INSET','MENU_GAP']),
        'Typography': ('typography', ['CODE_FONT_SIZE_DEFAULT','TERMINAL_FONT_SIZE_DEFAULT']),
    }
    for group, (key, names) in metric_specs.items():
        numbers[group] = []
        for name in names:
            m = re.search(r'\b(?:pub(?:\(crate\))?\s+)?const\s+' + name + r':\s*f32\s*=\s*([0-9.]+);', SOURCE[key])
            if not m:
                raise ValueError(f'Missing numeric metric {key}::{name}')
            numbers[group].append({'name': name, 'value': float(m[1]), 'unit': 'px', 'source': line_of(key, m.start())})
    specs = []
    for m in re.finditer(r'^pub const (\w+): MotionSpec = MotionSpec::new\((\d+),\s*(\w+)\)', SOURCE['motion'], re.M):
        specs.append({'name': m[1], 'duration': int(m[2]), 'curve': m[3], 'source': line_of('motion', m.start())})
    bezier = {m[1]: list(map(float, m.groups()[1:])) for m in re.finditer(r'^pub const (\w+): CubicBezier = CubicBezier::new\(([^,]+),\s*([^,]+),\s*([^,]+),\s*([^)]+)\)', SOURCE['motion'], re.M)}
    for motion in specs:
        motion['bezier'] = bezier.get(motion['curve'])
    return {
        'sourceRevision': fingerprint.hexdigest()[:12],
        'source': 'crates/ui/src/theme.rs', 'appearance': colors, 'accentPresets': accents,
        'groups': COLOR_ROLES, 'metrics': numbers, 'motion': specs,
        'fonts': [{'name':'Geist','source':'crates/ui/assets/fonts/Geist.ttf'}, {'name':'Geist Mono','source':'crates/ui/assets/fonts/GeistMono.ttf'}],
        'note': 'Default desktop themes and Zeron accent. Browser specimens approximate GPUI composition; dynamic imported themes are not enumerated.',
    }

if __name__ == '__main__':
    data = extract()
    text = json.dumps(data, indent=2) + '\n'
    if '--check' in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            raise SystemExit('styleguide.json is stale; run extract.py')
        print('Style guide is current.')
    else:
        OUT.write_text(text)
        print(f'Extracted {sum(map(len, COLOR_ROLES.values()))} color roles × 2, {sum(map(len, data["metrics"].values()))} metrics and {len(data["motion"])} motion specs.')
