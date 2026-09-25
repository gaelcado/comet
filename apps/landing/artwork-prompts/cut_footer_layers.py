"""Cut the aligned footer masters into transparent, topographic parallax planes.

Run from the repository root with Pillow and NumPy installed:
  python apps/landing/artwork-prompts/cut_footer_layers.py

The tracked wide WebPs are the masters. The cutout planes retain their exact
3840 x 1600 canvas, so every depth stays registered at any crop.
"""

from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter


ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / "apps/landing/public/assets/art"

# The seams follow the foothill crest and the near bank. During the reveal each
# closer plane starts higher, overlapping the one behind without duplicating it.
MIDDLE_RIDGE = [
    (0, 815), (180, 838), (380, 808), (610, 776), (780, 704),
    (980, 754), (1150, 789), (1320, 806), (1530, 810), (1690, 764),
    (1870, 822), (2090, 799), (2300, 762), (2470, 724), (2640, 747),
    (2800, 795), (3020, 804), (3220, 771), (3410, 793),
    (3630, 771), (3839, 748),
]
NEAR_BANK = [
    (0, 1090), (140, 1128), (310, 1190), (460, 1244), (620, 1260),
    (790, 1208), (1020, 1155), (1200, 1255), (1410, 1270),
    (1600, 1260), (1810, 1310), (2020, 1280), (2220, 1290),
    (2410, 1315), (2580, 1355), (2790, 1380), (2990, 1360),
    (3190, 1310), (3380, 1240), (3550, 1280), (3710, 1225),
    (3839, 1160),
]


def contour(points, width):
    xs, ys = zip(*points)
    return np.interp(np.arange(width), xs, ys)


def skyline(rgb, theme):
    """Find the first mountain pixel in each column, without its old sky."""
    if theme == "dark":
        terrain = (rgb[:, :, 0] > 13) & (rgb[:, :, 2] > 23)
    else:
        terrain = (rgb[:, :, 0] < 230) & (rgb[:, :, 2] < 220)
    # Consecutive pixels reject paper grain and isolated dark sky specks.
    count = sum(terrain[250 + offset:800 + offset] for offset in range(5))
    first = count.argmax(axis=0) + 252
    # A tiny horizontal median removes isolated threshold spikes while retaining
    # the fine, irregular mountain silhouette.
    padded = np.pad(first, (2, 2), mode="edge")
    return np.median(np.stack([padded[i:i + len(first)] for i in range(5)]), axis=0)


def tree_alpha(light_rgb, bank):
    """Carry the left foreground tree with its bank, including small branches."""
    height, width = light_rgb.shape[:2]
    y = np.arange(height)[:, None]
    x = np.arange(width)[None, :]
    # The tree is ink-dark on the paper master, while the lake is pale. The
    # dark master shares its silhouette closely enough to reuse this matte.
    ink = (light_rgb[:, :, 0] < 82) & (light_rgb[:, :, 1] < 84)
    region = (x > 210) & (x < 570) & (y > 1025) & (y < bank[None, :])
    mask = Image.fromarray(np.uint8(ink & region) * 255, "L")
    # Restore one-pixel twigs without growing a large dark halo.
    return np.asarray(mask.filter(ImageFilter.MaxFilter(3)))


def main():
    images = {
        theme: np.asarray(Image.open(DEST / f"footer-{theme}-wide.webp").convert("RGB"))
        for theme in ("dark", "light")
    }
    height, width = images["dark"].shape[:2]
    row = np.arange(height)[:, None]
    mid = contour(MIDDLE_RIDGE, width)
    bank = contour(NEAR_BANK, width)
    tree = tree_alpha(images["light"], bank)

    for theme, rgb in images.items():
        sky = skyline(rgb, theme)
        # Feather only the image's own antialiased skyline, never a broad band.
        top = np.clip((row - sky[None, :] + 1) * 170, 0, 255).astype(np.uint8)
        masks = {
            "far": np.where(row <= mid[None, :], top, 0).astype(np.uint8),
            "middle": np.where(
                (row >= mid[None, :]) & (row <= bank[None, :]),
                255, 0,
            ).astype(np.uint8),
            "near": np.maximum(np.where(row >= bank[None, :], 255, 0).astype(np.uint8), tree),
        }
        for plane, alpha in masks.items():
            rgba = np.dstack((rgb, alpha))
            output = DEST / f"footer-{theme}-{plane}.webp"
            Image.fromarray(rgba, "RGBA").save(output, "WEBP", quality=91, method=6)
            print(output.relative_to(ROOT), output.stat().st_size)


if __name__ == "__main__":
    main()
