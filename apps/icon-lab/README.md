# Zeron icon atelier · Study 02

135 original control glyphs, 36 state transitions, a preserved Study 01 design comparison, and separate small-size SVG exports. Gallery first: no native app source has been changed.

Open `index.html` directly, or serve this directory on localhost. No dependency installation or build is needed to view the gallery. Use **Refinements** for before/after comparisons, **The family** for the full inventory, and **In motion** for reversible playback and static scrubbing.

## Design direction

Clipped shoulders, continuous corners, generous open counters, and deliberate contour breaks. The Zeron workspace tile, robot, folders, documents and skills wand share the clearest family traits. Navigation and universal controls keep familiar silhouettes. Round caps and a 1.5-unit stroke tie them together on a 24-unit canvas.

Specific defects corrected include indistinguishable closed panel states, the accidental A project symbol, the closed-looking open folder, ambiguous file-style badge, crowded file-data marks, overprinted muted shapes, doubled save edges, and overly heavy grip dots. `CHANGES.md` lists each refined glyph and its reason.

The supplied Central Icons reference informed the original restrained line direction. All geometry here is custom-authored. No reference SVG artwork was downloaded or traced. Identity assets retain their existing files and licenses.

## Optical sizes

`svg/` contains the standard 24-unit drawings at a 1.5-unit stroke. `svg-small/` contains optical variants for 12–16px use, using a 1.75-unit stroke and simplified details where appropriate. Filled grip dots are separately sized to survive small rendering. The small files retain a 24-unit coordinate system; these are optical variants, not pixel-hinted 16-unit masters. The gallery automatically uses the small artwork at 16px and below. The inspector exports either version explicitly.

## Motion

`motion-geometry.js` is a pure geometry module, shared by the interactive gallery and rendered audit sheets. Matched contours use exact cubic Bézier interpolation with continuous De Casteljau subdivision, contour winding/start alignment, and exact original endpoints. Shared shapes are matched before contour indices. No uniform polyline resampling is used for motion.

Chevrons and sort controls rotate rigidly, avoiding flattened intermediate silhouettes. The eye closes anatomically. Unrelated symbols exchange visibility with a restrained 6% scale change while common outlines stay fixed; those are labelled stroke handoffs rather than geometric morphs. Both paths remain readable during the exchange. Filled stars and pins interpolate fill opacity.

All changes are interruptible from the current pose. OS reduced motion and the manual override snap to the destination. Hidden views stop scheduling animation frames. The timeline deliberately shows static poses, including under reduced motion. No autoplay, bounce, or animated theme colors. Native GPUI/SwiftUI integration remains a separate step.

## Sources and reproduction

- `build.py` + `polish.py`: original drawings, refinements, registry and symbol inventory. The source repository resolves to sibling `comet/`.
- `study-01.json`: the explicit initial design baseline for comparison.
- `catalog.json`, `inventory.json`, `INVENTORY.md`, `morphs.json`: geometry, mappings and state contexts.
- `motion-geometry.js`, `gallery.js`, `gallery.css`, `index.html`, `data.js`: standalone gallery.
- `verify.cjs`: geometry, endpoint and shared-contour regression checks.
- `render-sheets.cjs`: full contact sheet, selected comparisons and all 36 transitions at five poses.
- `package.py`: optical SVG exports, rendered sheets, verification and downloadable archive.

Rebuild with `python3 build.py`, then `python3 package.py`. Packaging needs Node and macOS `sips`; viewing needs only a browser.

The inventory is a source scan, not a runtime accessibility crawl. Current SHA and counts are recorded in `inventory.json`. All registered desktop controls, inspected iOS symbol literals/dispatch and contribution additions have targets. Provider logos, file/language identities, user-uploaded art and marketing artwork are outside this original-control redesign.
