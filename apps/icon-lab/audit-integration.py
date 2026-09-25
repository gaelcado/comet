"""Verify the icon study while keeping current app assets unchanged."""
from pathlib import Path
import json
import re
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parent
repo = root.parent.parent
catalog = json.loads((root / "catalog.json").read_text())
names = {icon["name"] for icon in catalog}
aliases = json.loads((root / "ios-symbols.json").read_text())
payload = json.loads((root / "proposals/ios/CustomIconLibrary.json").read_text())
assert payload["icons"] == [{key: icon[key] for key in ("name", "paths", "smallPaths")} for icon in catalog], "Stale iOS glyph export"
assert payload["aliases"] == aliases, "Stale iOS compatibility map"
assert set(aliases.values()) <= names
assert len(payload["motions"]) == len(json.loads((root / "motions.json").read_text()))
for motion in payload["motions"]:
    assert motion["from"] in names and motion["to"] in names
    assert len(motion["frames"]) == 97
    assert len(motion["smallFrames"]) == 97
for icon in catalog:
    for source, destination in [("svg", ""), ("svg-small", "small/")]:
        assert (root / source / (icon["name"] + ".svg")).read_bytes() == (
            repo / "crates/ui/assets/custom-icons" / (destination + icon["name"] + ".svg")
        ).read_bytes(), icon["name"]

# Every optical motion endpoint must be the exact static drawing it replaces.
native_bank = json.loads((repo / "crates/ui/assets/icon-motions.json").read_text())
for motion in native_bank:
    for field, prefix, stroke in [("frames", "", "1.75"), ("smallFrames", "small/", "1.75")]:
        frames = motion[field]
        assert len(frames) == 97
        for frame in frames:
            assert ET.fromstring(frame).attrib["stroke-width"] == stroke
        for index, name in [(0, motion["from"]), (96, motion["to"])]:
            static = ET.parse(repo / "crates/ui/assets" / name.replace("custom-icons/", "custom-icons/" + prefix))
            assert [p.attrib["d"] for p in ET.fromstring(frames[index])] == [
                p.attrib["d"] for p in static.getroot()
            ], (name, field, "optical endpoint mismatch")

for path in (repo / "apps/ios/Zeron").rglob("*.swift"):
    assert "ZeronIcon(" not in path.read_text(), path
assert not (repo / "apps/ios/Zeron/Theme/CustomIconLibrary.json").exists()
assert (root / "proposals/ios/ZeronIcon.swift").is_file()
assert (root / "proposals/ios/CustomIconTests.swift").is_file()

generated = (repo / "crates/ui/src/experimental_icons/generated.rs").read_text()
constants = re.findall(r'pub const (\w+): &str = "(.*?)";', generated)
for name, asset in constants:
    assert (repo / "crates/ui/assets" / asset).is_file(), (name, asset)
    assert asset.startswith("custom-icons/") or asset.endswith("-mark.svg") or asset.endswith("/zeron-logo.svg"), asset
native_source = (repo / "crates/ui/src/icons.rs").read_text()
assert "crate::file_icons::Assets.load(path)" in native_source
assert "custom-icons/" not in native_source
ui_source = (repo / "crates/ui/src/lib.rs").read_text()
assert '#[cfg(feature = "ui-workbench")]\npub mod experimental_icons;' in ui_source
print(f"PASS: {len(names)} proposed glyphs, {len(names)*2} SVGs, {len(constants)} desktop aliases, {len(aliases)} iOS aliases, {len(native_bank)} motion pairs.")
print("Scope: experimental desktop workbench and iOS proposal sources; current desktop and iOS controls retain production icon systems.")
