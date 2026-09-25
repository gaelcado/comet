# Landing landscape artwork

The four `*-wide.webp` files in `public/assets/art/` were generated with the
Image API (`gpt-image-2`, high quality, 3840 × 1600) from the prompts here.
`footer-dark` and `review-dark` use the previous site images as scene and style
references; each light image uses its new dark master as the geometry reference.
The full-resolution PNG masters are kept locally in `ignore/imagegen/`.

The footer master has no fixed sun. Its distinct mountain, lake, and near-bank
depth bands share the same source image in the page, with feathered masks and a
static base beneath them. This keeps all bands aligned at rest and hides gaps
during the small scroll offsets. Keep both theme variants compositionally
aligned if these assets are regenerated.
