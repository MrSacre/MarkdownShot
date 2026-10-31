import argparse
import json
import random
import shutil
import subprocess
import tempfile
from pathlib import Path

from playwright.sync_api import sync_playwright

HERE = Path(__file__).parent
PAGES = ["doc", "tuto", "notes"]
THEMES = {"light": "light", "dark": "dark"}
SCALES = {"100": 1.0, "125": 1.25}
VIEWPORT = {"width": 1280, "height": 760}

DOM_TO_MD = r"""
() => {
  let n = 0;
  const clean = s => s.replace(/\s+/g, ' ').trim();
  const inline = el => {
    let s = '';
    for (const c of el.childNodes) {
      if (c.nodeType === 3) { s += c.textContent; continue; }
      if (c.nodeType !== 1) continue;
      const t = c.tagName;
      if (['IMG', 'BUTTON', 'UL', 'OL', 'H4', 'P', 'PICTURE'].includes(t)) continue;
      if (t === 'STRONG' || t === 'B') s += '**' + clean(inline(c)) + '**';
      else if (t === 'CODE') s += '`' + c.textContent + '`';
      else s += inline(c);
    }
    return s;
  };
  const list = (el, indent) => {
    const lines = [];
    let i = 0;
    for (const li of el.children) {
      if (li.tagName !== 'LI') continue;
      i += 1;
      const marker = el.tagName === 'OL' ? `${i}.` : '-';
      const pad = indent + ' '.repeat(marker.length + 1);
      const h4 = li.querySelector(':scope > h4');
      const own = clean(inline(li));
      const head = h4 ? '#### ' + clean(inline(h4)) : own;
      lines.push(`${indent}${marker} ${head}`);
      for (const c of li.children) {
        if (c.tagName === 'P') lines.push(pad + clean(inline(c)));
        if (c.tagName === 'UL' || c.tagName === 'OL') lines.push(...list(c, pad));
      }
    }
    return lines;
  };
  const tableImg = () => `![[image-${++n}.png]]`;
  const figure = f => {
    const out = [];
    if (f.querySelector('img[data-figure]')) out.push(`![[image-${++n}.png]]`);
    const cap = f.querySelector('figcaption');
    if (cap) out.push(clean(cap.textContent));
    return out;
  };
  const blocks = [];
  for (const el of document.querySelector('main').children) {
    const t = el.tagName;
    if (/^H[1-6]$/.test(t)) blocks.push('#'.repeat(+t[1]) + ' ' + clean(inline(el)));
    else if (t === 'P' || (t === 'DIV' && el.classList.contains('meta'))) blocks.push(clean(inline(el)));
    else if (t === 'PRE') blocks.push('```\n' + el.textContent.replace(/\n+$/, '') + '\n```');
    else if (t === 'UL' || t === 'OL') blocks.push(list(el, '').join('\n'));
    else if (t === 'BLOCKQUOTE') blocks.push('> ' + clean([...el.querySelectorAll('p')].map(inline).join(' ')));
    else if (t === 'DIV' && el.classList.contains('callout')) blocks.push('> ' + clean(inline(el.querySelector('p'))));
    else if (t === 'FIGURE') blocks.push(...figure(el));
    else if (t === 'TABLE' && el.dataset.figure) blocks.push(tableImg());
    else if (t === 'TABLE') {
      const rows = [...el.querySelectorAll('tr')].map(tr => [...tr.children].map(c => clean(inline(c)).replace(/[|]/g, '\\|')));
      const n = Math.max(...rows.map(r => r.length));
      const line = r => '| ' + [...r, ...Array(n - r.length).fill('')].join(' | ') + ' |';
      blocks.push([line(rows[0]), '|' + ' --- |'.repeat(n), ...rows.slice(1).map(line)].join('\n'));
    }
    else if (t === 'DIV' && el.classList.contains('duo')) for (const f of el.querySelectorAll('figure')) blocks.push(...figure(f));
  }
  return blocks.join('\n\n') + '\n';
}
"""

FIGURES = r"""
() => [...document.querySelectorAll('main img[data-figure], main table[data-figure]')].map(img => {
  const r = img.getBoundingClientRect();
  return { kind: img.dataset.figure, src: (img.currentSrc || '').split('/').pop(),
           rect: [r.left + scrollX, r.top + scrollY, r.right + scrollX, r.bottom + scrollY] };
})
"""


def scroll_frames(page, out: Path, seed: int) -> int:
    rnd = random.Random(seed)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    total = page.evaluate("document.documentElement.scrollHeight - innerHeight")
    pos, i = 0, 0
    page.evaluate("scrollTo(0, 0)")
    while True:
        page.screenshot(path=out / f"{i:04d}.png")
        i += 1
        if rnd.random() < 0.15:
            page.screenshot(path=out / f"{i:04d}.png")
            i += 1
        if pos >= total:
            break
        pos = min(total, pos + rnd.randint(60, 420))
        page.evaluate(f"scrollTo(0, {pos})")
        page.wait_for_timeout(20)
    return i


def video(page, out_dir: Path, seed: int) -> int:
    rnd = random.Random(seed)
    total = page.evaluate("document.documentElement.scrollHeight - innerHeight")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        pos, k, speed = 0.0, 0, 0.0
        page.evaluate("scrollTo(0, 0)")
        for _ in range(10):
            page.screenshot(path=tmp / f"{k:05d}.png")
            k += 1
        while True:
            page.screenshot(path=tmp / f"{k:05d}.png")
            k += 1
            if pos >= total:
                for _ in range(10):
                    page.screenshot(path=tmp / f"{k:05d}.png")
                    k += 1
                break
            target = 0.0 if rnd.random() < 0.08 else rnd.uniform(8, 55)
            speed = 0.6 * speed + 0.4 * target
            pos = min(total, pos + speed)
            page.evaluate(f"scrollTo(0, {round(pos)})")
        mp4 = out_dir / "video.mp4"
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-framerate", "25", "-i", str(tmp / "%05d.png"),
             "-c:v", "libx264", "-preset", "veryfast", "-crf", "30", "-pix_fmt", "yuv420p", str(mp4)],
            check=True,
        )
    fv = out_dir / "frames_video"
    shutil.rmtree(fv, ignore_errors=True)
    fv.mkdir()
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", str(out_dir / "video.mp4"), "-vf", "fps=5", "-q:v", "4",
         str(fv / "%04d.jpg")],
        check=True,
    )
    return len(list(fv.iterdir()))


def run() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pages", default=",".join(PAGES))
    ap.add_argument("--themes", default=",".join(THEMES))
    ap.add_argument("--scales", default=",".join(SCALES))
    ap.add_argument("--no-video", action="store_true")
    a = ap.parse_args()
    with sync_playwright() as p:
        browser = p.chromium.launch()
        for name in a.pages.split(","):
            url = (HERE / f"{name}.html").resolve().as_uri()
            for theme in a.themes.split(","):
                for stag in a.scales.split(","):
                    scale = SCALES[stag]
                    out = HERE / "out" / f"{name}-{theme}-{stag}"
                    out.mkdir(parents=True, exist_ok=True)
                    ctx = browser.new_context(viewport=VIEWPORT, device_scale_factor=scale,
                                              color_scheme=THEMES[theme])
                    page = ctx.new_page()
                    page.goto(url)
                    page.wait_for_load_state("networkidle")
                    page.evaluate("document.fonts.ready")
                    page.screenshot(path=out / "full.png", full_page=True)
                    (out / "expected.md").write_text(page.evaluate(DOM_TO_MD), encoding="utf-8")
                    figs = page.evaluate(FIGURES)
                    for f in figs:
                        f["rect"] = [round(v * scale) for v in f["rect"]]
                    size = page.evaluate("[document.documentElement.scrollWidth, document.documentElement.scrollHeight]")
                    (out / "figures.json").write_text(
                        json.dumps({"scale": scale, "size": [round(size[0] * scale), round(size[1] * scale)],
                                    "images": figs}, indent=1),
                        encoding="utf-8",
                    )
                    seed = sum(map(ord, name + theme + stag))
                    nf = scroll_frames(page, out / "frames", seed)
                    nv = 0 if a.no_video else video(page, out, seed + 1)
                    print(f"{out.name:18} {len(figs)} images, {nf} scroll frames, {nv} video frames")
                    ctx.close()
        browser.close()


if __name__ == "__main__":
    run()
