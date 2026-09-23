# Native Zeron UI workbench

Run the standalone GPUI workbench from this checkout:

```sh
cargo run -p zeron -- ui-workbench
```

On macOS, `scripts/run-macos-dev.sh ui-workbench` builds and launches an isolated development bundle. The workbench does not start the engine or open a workspace. It renders through the same GPUI renderer and embedded asset source as Zeron.

## Fidelity contract

- Foundation swatches read the live `Theme` global. The toolbar cycles through installed theme variants, accents, and light/dark appearances without changing Zeron's saved settings. Typography uses the registered native font faces.
- Component cards call production functions in `settings::widgets`, `popover`, `surface_chrome`, `change_requests`, `changes`, `files`, `markdown`, `shell`, `notice`, `loaders`, `badges`, and `icons`. Their source builder is printed on each card. Data-bound views require a native fixture before they are marked mounted.
- The icon board loads the reviewed catalog of all 135 embedded regular and 16px optical SVGs through `icons::Assets`. It provides search, category filters, icon inspection, and all 36 transitions through `icons::icon(...).morph(...)` and the production native transition frames.
- `metrics.json` is generated metadata for numeric metrics and native motion timing; it is embedded for labels and provenance. Run `python3 apps/ui-workbench/extract.py --check` after editing Rust layout or motion values.
- `audit.json` is generated from all desktop UI Rust source files and the pattern catalog. It lists builder references, literal styling values, pointer affordances, and all scanned modules with source locations. Run `python3 apps/ui-workbench/audit.py --check` to verify it is current. Source occurrence counts are review signals, not defect claims.

`components.json` is a 34-entry review inventory. The workbench mounts 25 entries through production builders, with 28 live specimens including variants. Patterns & audit identifies eight app-bound patterns that need native fixture adapters or extraction of shared view builders, plus one status-treatment family that needs consolidation before it can be mounted with 1:1 fidelity. The source-wide module map also flags unmapped modules with public UI functions or `Render` implementations for further review; the catalog is not presented as exhaustive yet.

This is the desktop GPUI workbench. iOS uses SwiftUI and needs a separate native preview harness for its own components.
