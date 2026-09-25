# Landing landscape artwork

The four `*-wide.webp` masters were generated with the Image API (`gpt-image-2`,
high quality, 3840 × 1600). Their original prompts are in this directory.

## Footer parallax v2

The active footer uses `footer-{dark,light}-{distance,foreground}-v2.webp`.
Built-in imagegen edits produced the transparent cutouts and reconstructed lake;
WebP exports preserve alpha. Output canvases are approximately 1942 × 809.
These are separate complete plates, not bands cut out of one opaque image:

- Distance: mountains, far shore and a continuous lake extended behind the entire
  foreground. Sky removed along the skyline. No foreground bank or solitary tree.
- Foreground: exact near rocky shore silhouette, including pines and the solitary
  spreading tree. Lake and sky are transparent, including branch openings.
- The live glyph field supplies the sky behind both plates.

The foreground and its tree move as one rigid object. Scrolling down moves both
terrain plates up, with 4× more travel in the foreground (maximum 48 CSS pixels).
Both settle at y=0. Starting below that position keeps the bottom covered without
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
