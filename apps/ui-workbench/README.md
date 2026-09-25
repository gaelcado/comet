# Native Zeron UI workbench

Run the standalone GPUI workbench from this checkout:

```sh
cargo run -p zeron --features ui-workbench -- ui-workbench
```

On macOS, `scripts/run-macos-dev.sh ui-workbench` builds and launches an isolated development bundle. It does not start the engine or open a workspace.

## Current and target

**Current app** pages use the production `icons::Assets`, effective `Theme`, registered font faces and native builders. The navigation, page column, section labels and filled row blocks follow the current Settings system. Interactive specimens call production functions; data-bound screens are listed as needing a native fixture, not redrawn. The standalone workbench reads settings but does not change the saved appearance.

**Target studies** load `experimental_icons::Assets` only in the workbench. The proposed 135 regular SVGs, 135 optical SVGs and 36 reversible morphs are separate from production icon paths. A current/target comparison in the icon atelier makes that boundary visible. The `ui-workbench` feature is off in a standard Zeron build, so the experimental renderer and assets are absent from the production binary. Production startup still registers `icons::Assets` and retains today's glyphs and motion.

## Source coverage

`components.json` maps 111 current desktop patterns across foundations, actions, inputs, feedback, navigation, collections and composed surfaces. 33 have live native references; the remainder name the specific family and source file awaiting a faithful fixture or shared builder. `support.json` classifies 36 non-pattern modules. Together they account for all 112 scanned desktop UI Rust modules. `consolidation.json` records 12 open consolidation decisions with current implementations, source locations and proposed boundaries. These are review proposals, not product changes.

The generated `audit.json` records module classification, builder call sites and literal style signals. Counts are source occurrences rather than automatic defects. The audit excludes the workbench and experimental icon implementation from production counts. Source changes that introduce a module must classify it as a pattern or support module before the audit passes.

```sh
python3 apps/ui-workbench/audit.py --check
python3 apps/ui-workbench/extract.py --check
```

`metrics.json` is generated from current production layout and motion values for the Foundations page. The companion is desktop GPUI; iOS uses SwiftUI and needs its own native preview harness.
