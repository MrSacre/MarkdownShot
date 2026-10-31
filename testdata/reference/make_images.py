from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageFilter

HERE = Path(__file__).parent


def photo() -> None:
    w, h = 1200, 640
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=(int(250 - 120 * t), int(170 - 40 * t), int(90 + 110 * t)))
    d.ellipse([820, 120, 980, 280], fill=(255, 236, 170))
    for i, (col, base) in enumerate([((70, 90, 120), 380), ((45, 60, 85), 450), ((25, 35, 50), 520)]):
        pts = [(0, h)]
        for x in range(0, w + 1, 40):
            pts.append((x, base - int(70 * abs(((x / 170 + i) % 2) - 1))))
        pts.append((w, h))
        d.polygon(pts, fill=col)
    img = img.filter(ImageFilter.GaussianBlur(1.2))
    img.save(HERE / "mountains.jpg", quality=90)


def chart() -> None:
    w, h = 1100, 560
    img = Image.new("RGB", (w, h), (248, 250, 252))
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 22)
    bold = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 26)
    d.text((40, 24), "Temps de traitement par capture (ms)", font=bold, fill=(30, 41, 59))
    vals = [("Tesseract", 410), ("RapidOCR", 240), ("MarkdownShot", 180), ("VLM local", 9200)]
    maxv = 1000
    for i, (name, v) in enumerate(vals):
        y = 110 + i * 105
        d.text((40, y + 18), name, font=font, fill=(51, 65, 85))
        bw = int(min(v, maxv) / maxv * 780)
        d.rectangle([230, y, 230 + bw, y + 64], fill=(59, 130, 246) if name == "MarkdownShot" else (148, 163, 184))
        d.text((240 + bw, y + 18), f"{v}", font=font, fill=(30, 41, 59))
    img.save(HERE / "chart.png")


if __name__ == "__main__":
    photo()
    chart()
