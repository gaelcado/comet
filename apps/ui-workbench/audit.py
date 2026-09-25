"""Build the native workbench's source-usage audit.

This scans every production Rust file in crates/ui/src, excluding the
feature-gated workbench and experimental icon implementation. Counts are
source occurrences, not a claim that every
occurrence is a defect. The workbench labels them as consolidation candidates
and links to concrete locations for review.

Run: python3 apps/ui-workbench/audit.py [--check]
"""
from __future__ import annotations

import collections
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "crates/ui/src"
OUTPUT = Path(__file__).with_name("audit.json")
CATALOG = Path(__file__).with_name("components.json")
SUPPORT = Path(__file__).with_name("support.json")
CONSOLIDATION = Path(__file__).with_name("consolidation.json")

FAMILIES = {
    "Action builders": ["btn_primary", "btn_ghost", "btn_danger", "ghost_action", "toolbar_button"],
    "Input frames": ["input", "search_input_frame", "dialog_field"],
    "Popover surfaces": ["popover_card", "popover_card_flush", "palette_card", "dialog_card"],
    "Collection rows": ["card_row", "menu_row", "menu_row_nav", "render_tree_row", "queue_row"],
    "Status treatments": ["badge", "badge_active", "error_strip", "warning_strip", "pull_request_badge_preview"],
}

SIGNALS = {
    "literal-radii": (
        "Literal corner radii",
        r"\.rounded\(px\((\d+(?:\.\d+)?)\)\)",
        "The source uses several literal corner radii. Review them by surface and promote repeated semantic roles to shared metrics.",
    ),
    "literal-type": (
        "Literal type sizes",
        r"\.text_size\(px\((\d+(?:\.\d+)?)\)\)",
        "Literal text sizes coexist with the typography scale. Check whether each is an intentional optical adjustment or a missing type role.",
    ),
    "pointer-actions": (
        "Pointer affordances",
        r"\.cursor_pointer\(\)",
        "These elements advertise interaction. Review keyboard, role and focus behavior; the count alone does not imply a defect.",
    ),
    "explicit-button-roles": (
        "Explicit button roles",
        r"\.role\(gpui::Role::Button\)",
        "Compare with pointer affordances while auditing accessible interaction paths; GPUI controls can also supply roles themselves.",
    ),
}


def sources() -> dict[str, str]:
    return {
        path.relative_to(ROOT).as_posix(): path.read_text()
        for path in sorted(SOURCE.rglob("*.rs"))
        if path.name not in {"ui_workbench.rs", "experimental_icons.rs"}
        and "experimental_icons" not in path.parts
    }


def location(path: str, text: str, offset: int) -> str:
    return f"{path}:{text.count(chr(10), 0, offset) + 1}"


def occurrences(files: dict[str, str], expression: str) -> list[dict]:
    pattern = re.compile(expression)
    found = []
    for path, content in files.items():
        for match in pattern.finditer(content):
            line_start = content.rfind("\n", 0, match.start()) + 1
            line_end = content.find("\n", match.end())
            if line_end < 0:
                line_end = len(content)
            line = content[line_start:line_end]
            if line.lstrip().startswith("//"):
                continue
            found.append({
                "location": location(path, content, match.start()),
                "value": match.group(1) if match.lastindex else None,
            })
    return found


def call_sites(files: dict[str, str], symbol: str) -> list[str]:
    pattern = re.compile(r"\b" + re.escape(symbol) + r"\s*\(")
    found = []
    for path, content in files.items():
        for match in pattern.finditer(content):
            line_start = content.rfind("\n", 0, match.start()) + 1
            line_end = content.find("\n", match.end())
            if line_end < 0:
                line_end = len(content)
            line = content[line_start:line_end].strip()
            if line.startswith("//") or re.search(r"\bfn\s+" + re.escape(symbol) + r"\s*\(", line):
                continue
            found.append(location(path, content, match.start()))
    return found


def build() -> dict:
    files = sources()
    catalog = json.loads(CATALOG.read_text())
    support = json.loads(SUPPORT.read_text())
    consolidation = json.loads(CONSOLIDATION.read_text())
    ids = [item["id"] for item in catalog]
    if len(ids) != len(set(ids)):
        raise ValueError("component catalog contains duplicate IDs")
    for item in catalog:
        if not (ROOT / item["source"]).is_file():
            raise ValueError(f"missing source for {item['id']}: {item['source']}")
        if item["workbench"] not in {"mounted", "needs-fixture", "needs-consolidation"}:
            raise ValueError(f"invalid workbench status for {item['id']}")
    catalog_sources = collections.Counter(item["source"] for item in catalog)
    support_paths = {item["path"] for item in support}
    support_by_path = {item["path"]: item for item in support}
    if len(support_paths) != len(support):
        raise ValueError("duplicate support module")
    if support_paths & catalog_sources.keys():
        raise ValueError("a source module cannot be both a pattern and support")
    unclassified = set(files) - catalog_sources.keys() - support_paths
    stale = support_paths - set(files)
    if unclassified or stale:
        raise ValueError(f"source scope is incomplete: unclassified={sorted(unclassified)}, stale={sorted(stale)}")
    for item in consolidation:
        if item["priority"] not in {"High", "Medium", "Low"} or item["status"] != "open":
            raise ValueError(f"invalid consolidation decision: {item['id']}")
        for path in item["sources"]:
            if not (ROOT / path).is_file():
                raise ValueError(f"missing consolidation source: {path}")

    digest = hashlib.sha256()
    for path, content in files.items():
        digest.update(path.encode())
        digest.update(b"\0")
        digest.update(content.encode())
        digest.update(b"\0")
    digest.update(CATALOG.read_bytes())
    digest.update(SUPPORT.read_bytes())
    digest.update(CONSOLIDATION.read_bytes())

    families = []
    for name, symbols in FAMILIES.items():
        builders = []
        for symbol in symbols:
            sites = call_sites(files, symbol)
            builders.append({
                "symbol": symbol,
                "references": len(sites),
                "locations": sites[:8],
            })
        families.append({"name": name, "builders": builders})

    signals = []
    for key, (name, expression, note) in SIGNALS.items():
        hits = occurrences(files, expression)
        values = collections.Counter(hit["value"] for hit in hits if hit["value"])
        signals.append({
            "id": key,
            "name": name,
            "count": len(hits),
            "values": [{"value": value, "count": count} for value, count in values.most_common()],
            "locations": [hit["location"] for hit in hits[:12]],
            "note": note,
        })

    modules = []
    for path, content in files.items():
        public_functions = re.findall(r"(?m)^pub(?:\(crate\))?\s+fn\s+(\w+)", content)
        render_impls = re.findall(r"\bimpl\s+Render\s+for\s+(\w+)", content)
        modules.append({
            "path": path,
            "publicFunctions": len(public_functions),
            "renderImpls": len(render_impls),
            "exports": public_functions,
            "views": render_impls,
            "catalogEntries": catalog_sources[path],
            "role": "pattern" if path in catalog_sources else support_by_path[path]["role"],
            "scopeReason": None if path in catalog_sources else support_by_path[path]["reason"],
        })

    return {
        "sourceFingerprint": digest.hexdigest()[:16],
        "sourceFiles": len(files),
        "catalogedSourceFiles": sum(path in catalog_sources for path in files),
        "supportModules": len(support),
        "unreviewedSourceFiles": len(unclassified),
        "consolidationDecisions": len(consolidation),
        "catalogEntries": len(catalog),
        "mountedEntries": sum(item["workbench"] == "mounted" for item in catalog),
        "families": families,
        "signals": signals,
        "modules": modules,
    }


if __name__ == "__main__":
    output = json.dumps(build(), indent=2) + "\n"
    if "--check" in sys.argv:
        if not OUTPUT.exists() or OUTPUT.read_text() != output:
            raise SystemExit("audit.json is stale; run audit.py")
        print("Native source audit is current.")
    else:
        OUTPUT.write_text(output)
        print(f"Audited {json.loads(output)['sourceFiles']} Rust source files.")
