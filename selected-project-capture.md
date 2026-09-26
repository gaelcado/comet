# Selected-project before and after

Both images are native GPUI renderer captures of the same compact, dark sidebar fixture at 2200 × 1696 pixels. They use the same synthetic Fieldnotes project, session data, and `apps/landing/public/assets/zeron-favicon-v3.png` as the project's custom icon.

- `selected-project-before.png`: source commit `0e717118be77928bd5d926f0b2284d78e26fec5d`, the original sidebar-spacing tip. The fixture example alone was temporarily set to `settings.space_filter = Some("project".into())` so it would render the selected-project state. No production code was changed. The selected header shows the generic folder, while each session row repeats the custom project icon.
- `selected-project-after.png`: source commit `2d9d9a1e9b58af44ab1b71758adce4776d37f29b`, the PR head after the panel and project-icon fixes. The fixture's `ZERON_SIDEBAR_SELECTED_PROJECT=1` selects the same project. Its icon appears in the header, and the session rows no longer repeat it.

Build command: `cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture`. Capture settings: `ZERON_SIDEBAR_COMPACT=1`, `ZERON_SIDEBAR_PROJECT_ICON=<path to image>`, and `ZERON_SIDEBAR_CAPTURE_DIR=<output directory>`. The after capture also sets `ZERON_SIDEBAR_SELECTED_PROJECT=1`. These static captures verify the rendered state, not pointer interaction or animation.
