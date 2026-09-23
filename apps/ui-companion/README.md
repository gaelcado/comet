# Zeron UI companion

A local browser app for reviewing Zeron’s visual language. It combines an extracted desktop style guide, interactive component specimens, composed screen studies, and the full custom icon family with reversible motion. The [icon atelier](../icon-lab/README.md) remains the detailed glyph editor and audit gallery.

From the repository root:

```sh
python3 apps/ui-companion/extract.py
python3 -m http.server 8767 --bind 127.0.0.1
```

Open <http://127.0.0.1:8767/apps/ui-companion/>. No package install or build step is needed for the browser UI. Its bundled Geist faces and icon JSON are loaded from this repository. Use `python3 apps/ui-companion/extract.py --check` after changing the desktop theme or metrics; it fails if the checked-in reference has drifted. The generated `styleguide.json` is reviewed output, not a source to edit directly.

## What is source-backed

- 38 semantic color roles per appearance from `crates/ui/src/theme.rs`, including all seven desktop accent families. The extractor ports the source’s OKLCH-to-sRGB conversion for browser swatches and records each originating expression and line.
- 33 numeric metrics from the theme, surface chrome, composer, settings, popover and typography modules.
- 15 named native motion specifications and their easing names from `crates/ui/src/motion.rs`.
- 135 glyphs, two optical drawings, and 36 reversible icon transitions from `apps/icon-lab`.
- A curated catalog of 25 component specimens, each linked to its owning Rust module.

The browser specimens are intentionally **representative compositions**. They make hierarchy, state and theme iteration quick, but they do not execute GPUI or guarantee pixel identity. Imported user themes, terminal ANSI colors, OS glass/vibrancy and platform-specific layout are not enumerated by the extractor. Review changes in a native app build before treating them as runtime-verified.

## Working on the system

1. Edit the source of truth: desktop tokens and metrics in `crates/ui`, glyphs and motion in `apps/icon-lab`, and documented component specimens in this app.
2. Regenerate with `python3 apps/ui-companion/extract.py` and, for glyph changes, `python3 apps/icon-lab/build.py` followed by the existing native exporters.
3. Check the browser in dark and light, with multiple accents, keyboard focus, reduced motion and a narrow viewport.
4. Confirm behavior in GPUI/iOS for changes that affect the native apps.

The catalog is deliberately source linked. `components.json` lists the native owner for each specimen; `app.js` describes its browser composition. Add a new entry when a new reusable UI piece becomes important enough to compare across themes and states.
