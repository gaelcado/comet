# Landing landscape artwork

The four `*-wide.webp` masters were generated with the Image API (`gpt-image-2`,
high quality, 3840 × 1600). Their original prompts are in this directory.

## Footer parallax v3

The active footer uses `footer-{dark,light}-{peaks,foothills}-v3.webp` and
`footer-{dark,light}-foreground-v2.webp` in `public/assets/art/`.
Built-in imagegen edits produced the transparent cutouts and reconstructed lake;
WebP exports preserve alpha. Output canvases are approximately 1942 × 809.
These are separate complete plates, not bands cut out of one opaque image:

- Peaks: distant mountains with transparent sky. Near lakeside foothills and islands
  removed and their hidden scenery reconstructed as mist and uninterrupted lake.
- Foothills: low lakeside ridges, their pines, islands and lake. The upper edge
  follows their irregular topographic silhouette, with true transparency above.
- Foreground: exact near rocky shore silhouette, including pines and the solitary
  spreading tree. Lake and sky are transparent, including branch openings.
- The live glyph field supplies the sky behind all three plates.

The foreground and its tree move as one rigid object. Scrolling down moves all
three terrain plates up. Travel is 3%, 6.5% and 11.5% of landscape height, capped
at 18, 38 and 68 CSS pixels for peaks, foothills and foreground respectively.
A shared sine-out curve eases the layers into place, with a 0.45-second scroll
catch-up. The restrained foreground offset keeps the bank visible during reveal.
All settle at y=0. Starting below that position keeps the bottom covered without
scaling the art or repeating a bottom strip. Reduced motion uses the resting scene.

### Edit prompts (applied separately to both theme masters)

1. **Foreground extraction:** Keep only the closest rocky shore across the entire
   bottom with all its pine trees, especially the solitary spreading pine at 10%
   width. Preserve every branch and transparent opening. Remove lake, islands,
   far shore, mountains and sky to true alpha. Keep framing, colors and engraving.
2. **Clean distance plate:** Remove the entire closest bank and all its trees.
   Reconstruct matching horizontal engraved lake ripples continuously to the
   bottom. Preserve mountains, far shore, scale, framing and theme palette.
3. **Sky extraction:** Remove only the sky above the jagged mountain skyline to
   true alpha. Preserve all terrain, mist and lake below it without moving them.

Inspect transparency and branch edges against both light and dark surfaces, then
check overlap during the complete scroll reveal. The old polygon/threshold cutter has been removed: its broad seams and duplicated
tree caused artifacts.

### Additional v3 edit prompts (built-in imagegen, both themes)

4. **Foothill extraction:** Preserve canvas, registration, theme colors and lake.
   Keep only the nearer low lakeside foothills and all lake below to the bottom.
   Remove distant mountains and sky to true alpha. Follow the natural irregular
   ridge contour, including fine pine details; no horizontal band or feather.
   Use the dark cutout as the silhouette reference for the light variant.
5. **Reconstructed peaks:** Preserve upper mountain skyline, scale and palette.
   Remove the low foothills, islands and pines shown in the middle cutout.
   Reconstruct smooth misty lower slopes tapering into uninterrupted lake, with
   no repeated silhouettes of removed hills. Keep sky transparent and lake
   solid to the bottom.

Checked the three-layer composition in Chrome in light and dark themes, including
mid-reveal overlap. The foreground v2 tree extraction is retained without recutting.
