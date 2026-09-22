"""Guard the native custom-icon integration against legacy control fallbacks."""
from pathlib import Path
import json
import re

root = Path(__file__).resolve().parent
repo = root.parent.parent
catalog = json.loads((root / "catalog.json").read_text())
names = {icon["name"] for icon in catalog}
aliases = json.loads((root / "ios-symbols.json").read_text())
payload = json.loads((repo / "apps/ios/Zeron/Theme/CustomIconLibrary.json").read_text())
assert payload["icons"] == [{key: icon[key] for key in ("name", "paths", "smallPaths")} for icon in catalog], "Stale iOS glyph export"
assert payload["aliases"] == aliases, "Stale iOS compatibility map"
assert set(aliases.values()) <= names
assert len(payload["motions"]) == len(json.loads((root / "motions.json").read_text()))
for motion in payload["motions"]:
    assert motion["from"] in names and motion["to"] in names
    assert len(motion["frames"]) == 97
for icon in catalog:
    for source, destination in [("svg", ""), ("svg-small", "small/")]:
        assert (root / source / (icon["name"] + ".svg")).read_bytes() == (
            repo / "crates/ui/assets/custom-icons" / (destination + icon["name"] + ".svg")
        ).read_bytes(), icon["name"]

calls = 0
for path in (repo / "apps/ios/Zeron").rglob("*.swift"):
    source = path.read_text()
    assert not re.search(r"\b(?:Image|UIImage)\s*\(\s*systemName:", source), path
    assert not re.search(r"\b(?:Label|Button)\([^\n]*systemImage:", source), path
    for match in re.finditer(r'ZeronIcon\((?:systemName:\s*)?"([^"]+)"', source):
        assert match[1] in names or match[1] in aliases, (path, match[1])
        calls += 1
    for match in re.finditer(r'zeronIcon:\s*"([^"]+)"', source):
        assert match[1] in names or match[1] in aliases, (path, match[1])
        calls += 1
    # Include conditional literals and the explicitly named dynamic dispatch helpers.
    for line in source.splitlines():
        if "ZeronIcon(systemName:" in line or "iconButton(" in line or 'return "' in line and "TranscriptRows.swift" == path.name:
            for symbol in re.findall(r'"([a-z][a-z0-9.-]+)"', line.split('return ')[-1]):
                if 'return ' in line or "ZeronIcon(" in line or "iconButton(" in line:
                    assert symbol in names or symbol in aliases, (path, symbol)

generated = (repo / "crates/ui/src/icons/generated.rs").read_text()
constants = re.findall(r'pub const (\w+): &str = "(.*?)";', generated)
for name, asset in constants:
    assert (repo / "crates/ui/assets" / asset).is_file(), (name, asset)
    assert asset.startswith("custom-icons/") or asset.endswith("-mark.svg") or asset.endswith("/zeron-logo.svg"), asset
native_source = (repo / "crates/ui/src/icons.rs").read_text()
assert "file_icons::Assets" not in native_source
file_source = (repo / "crates/ui/src/file_icons.rs").read_text()
assert "crate::icons::icon(custom_icon_path(identity))" in file_source
assert "LineIconShape" not in (repo / "apps/ios/Zeron/Theme/LineIcons.swift").read_text()
print(f"PASS: {len(names)} shared glyphs, {len(names)*2} native SVGs, {len(constants)} desktop aliases, {len(aliases)} iOS aliases, {calls} static iOS icon calls.")
print("Scope: app-authored desktop/iOS controls and file glyphs. Logos, user imagery, system widgets and data/progress visualizations retain their renderers.")
