# Zeron icon atelier · Study 03

For the wider Zeron visual system, use the [native UI workbench](../ui-workbench/README.md).

135 original control glyphs, two optical sizes, and 36 reversible state transitions. The standalone study lives in `apps/icon-lab` on `design/custom-icon-library`. The feature-gated native workbench loads `crates/ui/src/experimental_icons` and `crates/ui/assets/custom-icons`. iOS proposal sources live in `apps/icon-lab/proposals/ios`; current desktop and iOS app icon systems remain unchanged.

Open `index.html`, or run `python3 -m http.server 8767 --bind 127.0.0.1` from this directory. Open http://localhost:8767. No dependencies are needed to view the gallery. **Refinements** compares Study 02 with 03; **The family** includes search, sizing and SVG exports; **In motion** supports playback, reversal and scrubbing; **Coverage** maps existing source names.

## Drawings

Continuous corners, clipped shoulders, open counters and deliberate contour breaks form the family. All geometry is custom authored; the Central Icons reference informed the restrained line direction without downloading or tracing reference SVGs. Existing provider and app brand identities retain their artwork and licenses.

Study 03 keeps the closed panel frames and gives open rails a short, inset travel. Slider tracks clear their circular knobs. Git states share node sizes and route anchors; draft dots extend into the ready route. Folder outlines, the wrench, lint, refresh, archive and muted controls receive further spacing corrections.

`glyphs.json` is the editable source. `svg/` uses a 1.75-unit stroke on a 24-unit canvas. `svg-small/` provides 1.75-unit optical variants and simplified details for 12–16px. These are optical variants, not pixel-hinted masters. The gallery uses them at 16px and below; its inspector exports either size.

## Motion

`motions.json` declares contour pairings and rigid rotation groups. `motion-geometry.js` produces exact SVG endpoints, matches cubic curves with De Casteljau subdivision, and preserves common frames. Arrows rotate as intact groups. Copy becomes a check, diff rows become an inset divider, and the eye closes along its lid. Unrelated contours use stroke erase/draw handoffs, labelled separately in the gallery.

`motion-state.js` uses critical damping and retains position and velocity when interrupted. Reduced motion snaps to the destination; hidden views stop their animation frames. Scrubbing intentionally exposes static intermediate poses. The workbench can preview the appearance morph; the current app retains its production transition.

## Build and verify

Run from this directory:

```sh
python3 build.py
node verify.cjs
python3 package.py
```

Python and Node use only their standard libraries. Packaging rebuilds and verifies before producing `zeron-icons.zip`, including the gallery, editable sources and all 270 SVGs. The generated archive is ignored by Git. To generate static audit sheets, run `node render-sheets.cjs`; on macOS, rasterize an individual sheet with `sips -s format png contact-sheet.svg --out contact-sheet.png`.

Verification covers both optical exports, stroke bounds, all 36 transitions at 101 poses, exact endpoints, shared contours, inset panel rails, subdivision fidelity, reversal continuity and frame-rate independence. Visual quality still requires inspection; these checks do not establish every possible rendering is defect-free.

`study-01.json` and `study-02.json` preserve comparison baselines. Generated `catalog.json`, `morphs.json` and `data.js` feed the gallery. Edit the canonical sources and rebuild instead of editing generated SVGs.

## Coverage

`inventory.json` records the source SHA and mappings. To rescan the containing Zeron repository and available contribution branches, use `python3 build.py --refresh-inventory`. This is a source scan of registered desktop assets, qualified Rust references, SwiftUI symbol literals and tool dispatch, not a runtime accessibility crawl. Dynamic image names need an integration audit. The proposed file and folder identities are cataloged alongside the control family; current app file icons remain in place. Provider/app logos, uploaded art, OS-owned chrome, progress visualizations, and website marketing artwork remain outside this control family.

## Native desktop experiment

Run `node export-native.cjs` after rebuilding the gallery to synchronize the proposed assets and aliases. The workbench-only `experimental_icons` module loads that bank and its 36 motion pairs. A standard Zeron build excludes the `ui-workbench` feature, so it does not include the experimental renderer or assets. Production `icons::Assets` and the file icon theme keep their current behavior.

The experimental `icon(path)` selects the small optical SVG at 16px or below. `.morph("state-glyph")` opts a glyph into element-local motion inside the workbench. Initial mounts, unrelated icon changes and reduced-motion settings render immediately. The 97-pose vector banks bound raster-cache identities while state progress remains interruptible.

```sh
cargo check -p zeron --features ui-workbench
cargo test -p zeron-ui --features ui-workbench --lib experimental_icons:: -- --test-threads=1
scripts/run-macos-dev.sh ui-workbench
```

## iOS proposal

Run `node export-ios.cjs` to generate `proposals/ios/CustomIconLibrary.json` from the same 135 glyphs and 36 motion banks. `proposals/ios/ZeronIcon.swift` and `CustomIconTests.swift` preserve the SwiftUI experiment for a future iOS fixture. They are outside the iOS app target and do not replace current SF Symbols or line icons.

Run `python3 audit-integration.py` to verify proposal exports, optical endpoints, desktop feature isolation and the absence of experimental icon calls in the current iOS app. This checks source boundaries and generated data; visual quality still needs native inspection when an iOS proposal harness exists.

Current weight trial: proposed glyphs and morphs use a uniform 1.75-unit stroke. The historical Study 02 comparison retains its original 1.5-unit stroke.
