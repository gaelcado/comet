# Native Zeron UI workbench

Run the standalone GPUI workbench from this checkout:

```sh
cargo run -p zeron -- ui-workbench
```

On macOS, `scripts/run-macos-dev.sh ui-workbench` builds and launches an isolated development bundle. The workbench does not start the engine or open a workspace. It renders through the same GPUI renderer and embedded asset source as Zeron.

## Fidelity contract

- Foundation swatches read the live `Theme` global. The toolbar cycles through installed theme variants, accents, and light/dark appearances without changing Zeron's saved settings. Typography uses the registered native font faces.
- Component cards call production functions in `settings::widgets`, `popover`, `surface_chrome`, `change_requests`, `changes`, and `icons`. Their source path is printed on each card. The workbench does not draw HTML/CSS replicas of data-bound views.
- The icon board lists all 135 embedded regular and 16px optical SVGs through `icons::Assets`; all 36 transitions use `icons::icon(...).morph(...)` and the production native transition frames.
- `metrics.json` is generated metadata for numeric metrics and native motion timing; it is embedded for labels and provenance. Run `python3 apps/ui-workbench/extract.py --check` after editing Rust layout or motion values.

`components.json` is a 25-entry review inventory. The workbench currently mounts 9 catalog entries through their production builders, plus several shared primitives that are not separate catalog entries. Its Coverage page identifies the remaining 16 app-level composites that need fixture adapters or extraction of their shared view builders before they can be shown with 1:1 fidelity. They are not represented by lookalikes. The inventory is therefore a coverage target, not a claim of complete component coverage.

This is the desktop GPUI workbench. iOS uses SwiftUI and needs a separate native preview harness for its own components.
