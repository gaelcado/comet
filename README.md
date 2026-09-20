# Sidebar fixture evidence

Source: 4215ad40cdfce3c4c6e9d74c6eec827d99b10021. Parent asset revision b8561debddeb84fae4afeca9cc4457d285145c66 retains earlier captures.
Build: cargo build -p zeron-ui --example sidebar-fixture --features project-palette-fixture,appshots-fixture
Executable SHA256: cd20080720c98b346effaf003a5267bc5ef0f3769b2aa603eca565562a10380b

Native GPUI readbacks of production Shell with synthetic data, compact rows, 310 px sidebar, dark/light frosted appearance. Requested window 1100×1000, constrained to native screen bounds. OS window decoration and composited desktop backdrop excluded.

States: collapsed, one-list, by-project, by-device, hover-actions, project-icon-menu. Working and completed sessions show status instead of elapsed time; idle rows show time. Project icons precede harness icons. Native upload picker cancel/apply/reset tested through GPUI, not a screenshot of the operating-system picker.

124 shell tests and settings round-trip test passed. Captures do not establish motion smoothness. Before screenshot was supplied from the earlier fixture; window size differs.
