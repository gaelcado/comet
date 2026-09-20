# Sidebar fixture evidence

Source `e84f0415e7f3e63e627fd2868b10b2d644e58731`, sidebar-spacing branch, clean source matching the captured build.

Build: `cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture`. Executable SHA256 `04b66506dc7bd38000959591629c24bd3f04ff64e0c9e28ed3e1203623d3ca5a`.

Website custom icon: https://zeron.sh/assets/zeron-favicon-v3.png, applied to Fieldnotes. Remote project uses its monogram.

Capture with `ZERON_SIDEBAR_COMPACT=1 ZERON_SIDEBAR_PROJECT_ICON=/path/to/zeron-favicon-v3.png ZERON_SIDEBAR_CAPTURE_DIR=/tmp/sidebar-captures`; add `ZERON_PALETTE_LIGHT=1` for light, omit COMPACT for expanded. Isolated temporary data, IPC 0, 21 synthetic sessions.

Inspected dark compact normal/hover, light project grouping and dark expanded rows. New grouping: 6px identity-icon gap, 8px identity-to-title, 12px title-to-metadata, 8px metadata gaps, 24px minimum trailing slot, 24px action targets with 4px gap. Disclosure chevrons centered in matching 24px slot.

Validation: 124 shell tests pass, fixture build and scoped formatting/diff checks pass. GPUI readbacks exclude OS decorations/backdrop and do not prove animation smoothness. Original before image is unchanged and has different window dimensions.
