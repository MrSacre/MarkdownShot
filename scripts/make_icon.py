from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "logo-source.png"

BOX = (202, 194, 1042, 1014)
RADIUS = 196
BG = (7, 5, 15)
GLOW = (124, 58, 255)


def rounded_mask(size, box, radius, scale=4):
    w, h = size
    m = Image.new("L", (w * scale, h * scale), 0)
    x0, y0, x1, y1 = (v * scale for v in box)
    ImageDraw.Draw(m).rounded_rectangle((x0, y0, x1, y1), radius * scale, fill=255)
    return m.resize(size, Image.LANCZOS)


def tight_icon(src):
    mask = rounded_mask(src.size, BOX, RADIUS)
    rgba = src.convert("RGBA")
    rgba.putalpha(mask)
    x0, y0, x1, y1 = BOX
    side = max(x1 - x0, y1 - y0) + 2
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    crop = (round(cx - side / 2), round(cy - side / 2), round(cx + side / 2), round(cy + side / 2))
    return rgba.crop(crop)


def glow_logo(src):
    rgb = src.convert("RGB")
    w, h = rgb.size
    inner = rounded_mask((w, h), BOX, RADIUS)
    px = rgb.load()
    alpha = Image.new("L", (w, h), 0)
    color = Image.new("RGB", (w, h), GLOW)
    ap, cp = alpha.load(), color.load()
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            a = max((p[i] - BG[i]) / max(GLOW[i] - BG[i], 1) for i in range(3))
            a = min(max((a - 0.04) / 0.96, 0.0), 1.0)
            ap[x, y] = int(a * 255)
            if a > 0.02:
                cp[x, y] = tuple(min(255, max(0, round((p[i] - (1 - a) * BG[i]) / a))) for i in range(3))
    halo = color.convert("RGBA")
    halo.putalpha(alpha)
    body = rgb.convert("RGBA")
    body.putalpha(inner)
    out = Image.alpha_composite(halo, body)
    m = 130
    x0, y0, x1, y1 = BOX
    side = max(x1 - x0, y1 - y0) + 2 * m
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    return out.crop((round(cx - side / 2), round(cy - side / 2), round(cx + side / 2), round(cy + side / 2)))


def main():
    src = Image.open(SRC)
    icon = tight_icon(src)
    icon.resize((256, 256), Image.LANCZOS).save(ROOT / "assets" / "markdownshot.png", optimize=True)
    sizes = [256, 128, 64, 48, 40, 32, 24, 20, 16]
    frames = [icon.resize((s, s), Image.LANCZOS) for s in sizes]
    frames[0].save(
        ROOT / "assets" / "markdownshot.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[1:]
    )
    glow_logo(src).resize((512, 512), Image.LANCZOS).save(ROOT / "assets" / "logo.png", optimize=True)
    print("icons written to assets/")


if __name__ == "__main__":
    main()
