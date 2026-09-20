# Sidebar fixture evidence

Source `0e717118be77928bd5d926f0b2284d78e26fec5d`, branch sidebar-spacing. Build: `cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture`. Binary SHA256 `758650c77eab41be3e8b28f5ff56cc4f1fb73ae7def67e77cb8fcbb9d7bee0f2`.

Latest dark/light captures use the website icon https://zeron.sh/assets/zeron-favicon-v3.png as the custom Fieldnotes project icon. Run with `ZERON_SIDEBAR_COMPACT=1 ZERON_SIDEBAR_PROJECT_ICON=/path/to/zeron-favicon-v3.png ZERON_SIDEBAR_CAPTURE_DIR=/tmp/sidebar-captures`; add `ZERON_PALETTE_LIGHT=1` for light. Isolated temporary state, IPC 0, 21 synthetic sessions.

Action controls retain 24px hit targets, with centered 20px painted surfaces and 14px glyphs, 2px between hit targets (6px between painted surfaces). A 4px trailing hit-area extension leaves 6px painted clearance from the row edge. The 29px row has 4.5px above and below the painted surfaces. Native archive hover visually inspected through CUA; pure renderer captures do not include pointer or OS backdrop. Fixture engine is disconnected, so archive operations cannot be persisted there.

124 shell tests passed, including target/surface centering, containment and pointer stability. Build and diff/format checks passed. Existing expanded captures are from e84f0415 and retained as earlier evidence; all dark/light after images are current. Before image unchanged. Static captures do not validate motion smoothness.
