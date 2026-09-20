# Sidebar fixture evidence

Source: `f5c6a0dcc8c0dab7af1d0a2e4d15b22d8b40e5ca` on `sidebar-spacing`.

Built with `cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture`. Binary SHA256: `1cc296d4a829427fa914f06c8ec9a9715fe06287caae4fdb9cbf71dec7778318`.

All after captures use https://zeron.sh/assets/zeron-favicon-v3.png as the Fieldnotes custom project icon (SHA256 `f935d754ee5c3a26aa8a9653fbf12c5478b84cda8f28dea4921fdca73dcf3f6c`). The remote API server retains its fallback monogram.

Run with `ZERON_SIDEBAR_COMPACT=1 ZERON_SIDEBAR_PROJECT_ICON=/path/to/zeron-favicon-v3.png ZERON_SIDEBAR_CAPTURE_DIR=/tmp/sidebar-captures`; add `ZERON_PALETTE_LIGHT=1` for light. The fixture seeds the persisted override in isolated temporary data, IPC port 0. Native GPUI renderer readbacks, 21 synthetic sessions. OS decorations/backdrop are excluded; static images do not validate motion. Original before-collapsed image retained unchanged; its window size differs.

Inspected dark rows and changed-icon menu, plus light project grouping. Fixture build and diff checks pass; production logic unchanged from 4215ad40 (124 shell tests and settings round-trip passed there).
