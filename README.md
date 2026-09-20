# Sidebar fixture review evidence

Source: 88c43296225c1936ca496ed14ba19acfe316dca5, branch sidebar-spacing, based on origin/main 6ecea055877e1f560adbc4545b8d8fd7dffa6d75.
Build: cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture
Binary SHA256: f6e02e0c459a63b640508a04cde699e1ce67c2d1d1846dd772e41d0a48123d97
Captured: 2026-09-20T09:49:38.949891+00:00

After images are production GPUI window render readbacks with synthetic data, compact layout, 310 px sidebar, requested 1100×1000 window (native screen bounds may constrain height), frosted surface in dark/light modes. OS window decorations/backdrop are not part of the renderer capture. Hover state is set by the fixture; real pointer transitions are covered by GPUI tests.

Before image is the reporter's screenshot of the earlier isolated sidebar fixture, before uniform section gaps. Different window size; compare the relative spacing between collapsed headers.

Validated: all 33 pinned_session_tests, scoped rustfmt, git diff --check. Captured collapsed, one-list, by-project, by-device, hover-actions in both appearances. No pixel-perfect opaque/minimum-width or motion-smoothness claim.
