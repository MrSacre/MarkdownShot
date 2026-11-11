#!/usr/bin/env python3
import argparse
import glob
import os
import random
import string
import sys
from multiprocessing import Pool

import numpy as np
from PIL import Image, ImageDraw, ImageFont

W, H = 24, 32
SPAN_X = 1.0
TOP, BOTTOM = 1.9, 0.7
CHARS = [chr(c) for c in range(32, 127)]
INDEX = {c: i for i, c in enumerate(CHARS)}
SYMBOLS = string.punctuation
HOLDOUT = ("cascadia-code", "roboto-mono", "ibm-plex-mono")
SS = 4


def patches(img: np.ndarray, bg: np.ndarray, centers: np.ndarray, baseline: float, pitch: float) -> np.ndarray:
    dist = np.sqrt(((img.astype(np.float32) - bg.astype(np.float32)) ** 2).sum(axis=2))
    ih, iw = dist.shape
    u = (np.arange(W, dtype=np.float32) + 0.5) / W
    v = (np.arange(H, dtype=np.float32) + 0.5) / H
    xs = centers[:, None] - SPAN_X * pitch + u[None, :] * (2 * SPAN_X * pitch) - 0.5
    ys = baseline - TOP * pitch + v * ((TOP + BOTTOM) * pitch) - 0.5
    x0 = np.floor(xs).astype(np.int64)
    y0 = np.floor(ys).astype(np.int64)
    fx = (xs - x0).astype(np.float32)
    fy = (ys - y0).astype(np.float32)

    def at(yy, xx):
        ok = (yy >= 0) & (yy < ih)
        yy = np.clip(yy, 0, ih - 1)
        okx = (xx >= 0) & (xx < iw)
        xx = np.clip(xx, 0, iw - 1)
        g = dist[yy[None, :, None], xx[:, None, :]]
        return g * ok[None, :, None] * okx[:, None, :]

    a = at(y0, x0) * (1 - fx)[:, None, :] + at(y0, x0 + 1) * fx[:, None, :]
    b = at(y0 + 1, x0) * (1 - fx)[:, None, :] + at(y0 + 1, x0 + 1) * fx[:, None, :]
    out = a * (1 - fy)[None, :, None] + b * fy[None, :, None]
    m = out.reshape(len(centers), -1).max(axis=1)
    return out / np.maximum(m, 1.0)[:, None, None], m


def random_text(rnd: random.Random, n: int) -> str:
    out = []
    while len(out) < n:
        r = rnd.random()
        if r < 0.16:
            out += [rnd.choice("^-=.|_~*#+/\\<>:`'\"")] * rnd.randint(2, 9)
        elif r < 0.30:
            out += [" "] * rnd.randint(1, 5)
        elif r < 0.60:
            out += rnd.choices(string.ascii_lowercase, k=rnd.randint(1, 8))
        elif r < 0.70:
            out += rnd.choices(string.ascii_uppercase, k=rnd.randint(1, 5))
        elif r < 0.78:
            out += rnd.choices(string.digits, k=rnd.randint(1, 5))
        else:
            out += rnd.choices(SYMBOLS, k=rnd.randint(1, 3))
    return "".join(out[:n])


def color(rnd: random.Random, dark_bg: bool, ink: bool) -> np.ndarray:
    if ink == dark_bg:
        base = rnd.uniform(150, 255)
        c = [min(255, max(0, base + rnd.uniform(-70, 40))) for _ in range(3)]
        if rnd.random() < 0.5:
            c = [base + rnd.uniform(-8, 8)] * 3
    else:
        base = rnd.uniform(0, 70) if ink else rnd.uniform(8, 60)
        c = [min(255, max(0, base + rnd.uniform(-10, 60 if ink else 14))) for _ in range(3)]
    return np.array(c, dtype=np.float32).clip(0, 255)


def render(rnd: random.Random, font_path: str) -> tuple[np.ndarray, np.ndarray]:
    size = rnd.uniform(10.5, 26.0)
    font = ImageFont.truetype(font_path, max(8, round(size * SS)))
    adv = font.getlength("M") / SS
    if rnd.random() < 0.35:
        adv = max(4.0, round(adv))
    n = rnd.randint(18, 46)
    dy = adv * rnd.uniform(1.85, 3.3)
    rows = [random_text(rnd, n) for _ in range(3)]
    margin = int(adv * 2) + 4
    w = int(n * adv) + 2 * margin
    h = int(dy * 3 + adv * 2) + 8
    dark = rnd.random() < 0.5
    bg = color(rnd, dark, False)
    img = np.empty((h, w, 3), dtype=np.float32)
    img[:] = bg
    lcd = rnd.random() < 0.5
    gamma = rnd.uniform(0.75, 1.35)
    x_start = margin + rnd.uniform(0, 1)
    base0 = adv * 2 + rnd.uniform(0, 1) + 2
    k = 0
    for r, text in enumerate(rows):
        by = base0 + r * dy
        i = 0
        while i < n:
            j = min(n, i + rnd.randint(1, 9))
            fg = color(rnd, dark, True)
            y_a = max(0, int(by - 2.3 * adv))
            y_b = min(h, int(by + 1.1 * adv) + 1)
            bh = y_b - y_a
            mask = Image.new("L", (w * SS, bh * SS), 0)
            d = ImageDraw.Draw(mask)
            for q in range(i, j):
                if text[q] != " ":
                    d.text((round((x_start + q * adv) * SS), round((by - y_a) * SS)), text[q], font=font, fill=255, anchor="ls")
            m = np.asarray(mask, dtype=np.float32) / 255.0
            m = m.reshape(bh, SS, w * SS).mean(axis=1)
            if lcd:
                pad = np.pad(m, ((0, 0), (SS, SS)))
                cov = np.empty((bh, w, 3), dtype=np.float32)
                for c in range(3):
                    off = round((c - 1) * SS / 3)
                    sub = pad[:, SS + off - 1: SS + off - 1 + w * SS + 2]
                    box = (sub[:, :-2] + sub[:, 1:-1] + sub[:, 2:]) / 3
                    cov[:, :, c] = box.reshape(bh, w, SS).mean(axis=2)
            else:
                cov = np.repeat(m.reshape(bh, w, SS).mean(axis=2)[:, :, None], 3, axis=2)
            cov = cov.clip(0, 1) ** gamma
            img[y_a:y_b] = img[y_a:y_b] * (1 - cov) + fg[None, None, :] * cov
            i = j
        k += 1
    if rnd.random() < 0.15:
        img += np.random.default_rng(rnd.randrange(1 << 30)).normal(0, rnd.uniform(0.5, 3.0), img.shape).astype(np.float32)
    img = img.clip(0, 255).round()
    text = rows[1]
    by = base0 + dy
    pitch = adv * (1 + rnd.uniform(-0.012, 0.012))
    centers = x_start + (np.arange(n, dtype=np.float32) + 0.5) * adv + rnd.uniform(-0.14, 0.14) * adv
    centers = centers + np.array([rnd.uniform(-0.05, 0.05) * adv for _ in range(n)], dtype=np.float32)
    bg_est = bg + np.array([rnd.uniform(-1.5, 1.5) for _ in range(3)], dtype=np.float32)
    x, m = patches(img, bg_est.round(), centers, by + rnd.uniform(-0.24, 0.24) * adv, pitch)
    y = np.array([INDEX[c] for c in text], dtype=np.int64)
    keep = m >= 12.0
    return (x[keep] * 255).round().astype(np.uint8), y[keep]


def work(args):
    seed, fonts, lines = args
    rnd = random.Random(seed)
    xs, ys = [], []
    for _ in range(lines):
        try:
            x, y = render(rnd, rnd.choice(fonts))
        except Exception as e:
            print("skip", e, file=sys.stderr)
            continue
        xs.append(x)
        ys.append(y)
    return np.concatenate(xs), np.concatenate(ys)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fonts", default="/home/claude/trainfonts")
    ap.add_argument("--out", default="/home/claude/glyphdata")
    ap.add_argument("--lines", type=int, default=16000)
    ap.add_argument("--val-lines", type=int, default=1500)
    ap.add_argument("-j", type=int, default=2)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    files = sorted(glob.glob(os.path.join(a.fonts, "*.ttf")))
    held = [f for f in files if os.path.basename(f).startswith(HOLDOUT)]
    train = [f for f in files if f not in held]
    print(len(train), "training fonts,", len(held), "held out")
    for name, fonts, lines, seed in [("train", train, a.lines, 1000), ("val_seen", train, a.val_lines, 5), ("val_unseen", held, a.val_lines, 7)]:
        chunk = 200
        jobs = [(seed * 100000 + i, fonts, chunk) for i in range(max(1, lines // chunk))]
        with Pool(a.j) as pool:
            parts = pool.map(work, jobs)
        x = np.concatenate([p[0] for p in parts])
        y = np.concatenate([p[1] for p in parts])
        np.savez_compressed(os.path.join(a.out, f"{name}.npz"), x=x, y=y)
        print(name, x.shape)


if __name__ == "__main__":
    main()
