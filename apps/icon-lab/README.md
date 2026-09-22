# Zeron icon atelier · Study 03

135 original control glyphs, two optical sizes, and 36 reversible state transitions. The standalone gallery lives in `apps/icon-lab` on `design/custom-icon-library`. The desktop integration lives in `crates/ui/src/icons` and `crates/ui/assets/custom-icons`. iOS uses the same vector source through SwiftUI Canvas and custom UIKit menu images.

Open `index.html`, or run `python3 -m http.server 8767 --bind 127.0.0.1` from this directory. Open http://localhost:8767. No dependencies are needed to view the gallery. **Refinements** compares Study 02 with 03; **The family** includes search, sizing and SVG exports; **In motion** supports playback, reversal and scrubbing; **Coverage** maps existing source names.

## Drawings

Continuous corners, clipped shoulders, open counters and deliberate contour breaks form the family. All geometry is custom authored; the Central Icons reference informed the restrained line direction without downloading or tracing reference SVGs. Existing provider and app brand identities retain their artwork and licenses.

Study 03 keeps the closed panel frames and gives open rails a short, inset travel. Slider tracks clear their circular knobs. Git states share node sizes and route anchors; draft dots extend into the ready route. Folder outlines, the wrench, lint, refresh, archive and muted controls receive further spacing corrections.

`glyphs.json` is the editable source. `svg/` uses a 1.5-unit stroke on a 24-unit canvas. `svg-small/` provides 1.75-unit optical variants and simplified details for 12–16px. These are optical variants, not pixel-hinted masters. The gallery uses them at 16px and below; its inspector exports either size.

## Motion

`motions.json` declares contour pairings and rigid rotation groups. `motion-geometry.js` produces exact SVG endpoints, matches cubic curves with De Casteljau subdivision, and preserves common frames. Arrows rotate as intact groups. Copy becomes a check, diff rows become an inset divider, and the eye closes along its lid. Unrelated contours use stroke erase/draw handoffs, labelled separately in the gallery.

`motion-state.js` uses critical damping and retains position and velocity when interrupted. Reduced motion snaps to the destination; hidden views stop their animation frames. Scrubbing intentionally exposes static intermediate poses. The theme button uses the same appearance morph.

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

`inventory.json` records the source SHA and mappings. To rescan the containing Zeron repository and available contribution branches, use `python3 build.py --refresh-inventory`. This is a source scan of registered desktop assets, qualified Rust references, SwiftUI symbol literals and tool dispatch, not a runtime accessibility crawl. Dynamic image names need an integration audit. File and folder identities use the custom family too. Provider/app logos, uploaded art, OS-owned chrome, progress visualizations, and website marketing artwork remain outside this control family.

## Native desktop integration

Run `node export-native.cjs` after rebuilding the gallery to synchronize native assets and constants. Existing control constant names remain aliases for the new family. Provider marks and the app logo retain their assets. File/folder rendering uses the custom family; the old file-theme manifest remains only for recognizing filename references and its assets are no longer served by the app.

`icons::icon(path)` selects the small optical SVG for static glyphs at 16px or below. `.morph("state-glyph")` opts a glyph into element-local motion; put it inside a control with a stable unique ID. Stateful glyphs retain the standard master at rest and in motion to avoid an optical-size jump. Initial mounts, unrelated icon changes and OS reduced motion render immediately. Unmounted elements stop requesting frames and their state expires with GPUI's frame state.

The native bank contains the 36 reviewed transitions plus system-to-light for the three-way appearance selector. Each uses 97 vector poses from the gallery engine, selected by an interruptible critically damped progress value. This bounds SVG raster-cache identities; temporal progress is continuous while geometry is quantized to 1/96. At rest, exact endpoint SVGs are used.

Connected controls include appearance navigation, left/right panes, pane expansion, unified/split diffs, wrapping, fold/disclosure controls, hidden files, favorites, copy confirmation, PR merged state, queue actions and composer send/stop. The protocol currently exposes open/closed/merged PR states, not draft; draft geometry remains available for future data support. Additional gallery concepts do not create new app behaviors automatically.

Validate with `cargo check -p zeron-ui --lib` and `cargo test -p zeron-ui --lib icons:: -- --test-threads=1`. Runtime review should use an isolated development app, never the production instance hosting the coding session.


## iOS integration and coverage guard

Run `node export-ios.cjs` to generate the SwiftUI payload from the same 135 glyphs and 36 motion banks. `ios-symbols.json` is the explicit compatibility map for existing call-site names. `ZeronIcon` draws custom paths, uses both optical sizes, and animates supported state pairs with an interruptible spring. Reduced motion snaps. Menu labels use template images rasterized from the same paths because UIKit menus require an Image.

Run `python3 audit-integration.py` to check the two native exports, compatibility mappings, static Swift call sites, and absence of legacy SF Symbol rendering. Dynamic tool dispatch remains explicit in the inventory. This guard covers source integration; it does not establish runtime frame timing or replace interactive UI review.

The iOS `CustomIconTests` decode every contour and motion frame, render all glyphs at 12/16/24px, and exercise every menu alias. App-owned compact loading indicators and Markdown task glyphs also use this family; branded loading artwork, determinate progress, and native system widgets retain their own rendering.
