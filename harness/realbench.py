#!/usr/bin/env python3
import argparse
import functools
import http.server
import json
import os
import random
import re
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "harness"))
import eval as ev

BENCH = Path(os.environ.get("BENCH", "/home/claude/bench"))
OUT = ROOT / "target" / "realbench"
EXE = ROOT / "target" / "release" / ("markdownshot.exe" if os.name == "nt" else "markdownshot")

GEN = {
    "mdbook": {"content": "main"},
    "mkdocs": {"content": "article.md-content__inner"},
    "sphinx": {"content": "div[role=main], div.body, article"},
    "docusaurus": {"content": "article"},
    "vitepress": {"content": "main"},
}
SITES = {
    "rust-book": "mdbook",
    "rbe": "mdbook",
    "mkdocs-material": "mkdocs",
    "godot-docs": "sphinx",
    "rn-website": "docusaurus",
    "flask": "sphinx",
    "python-docs": "sphinx",
    "fastapi": "mkdocs",
    "cp-algorithms": "mkdocs",
    "vue-docs": "vitepress",
}
INCLUDE = {
    "rn-website": r"^(docs/[^/]+\.html|blog/\d{4}/.+\.html)$",
    "fastapi": r"^(?!_llm-test|release-notes|fastapi-people|management|translations|external-links|newsletter).+\.html$",
    "python-docs": r"^(tutorial|howto|library|faq|reference)/.+\.html$",
    "cp-algorithms": r"^(?!index|navigation|preview|tags|contrib|code_of_conduct).+\.html$",
}
VARIANTS = {"light-100": ("light", 1.0), "dark-100": ("dark", 1.0), "light-125": ("light", 1.25), "dark-125": ("dark", 1.25)}
VIEWPORT = {"width": 1280, "height": 800}
MAX_H = 12000

TRUTH = r"""
({ sel, ylim }) => {
  const root = [...document.querySelectorAll(sel)].sort((a, b) => b.getBoundingClientRect().height - a.getBoundingClientRect().height)[0];
  if (!root) return null;
  const vis = el => {
    if (el.checkVisibility && !el.checkVisibility({ opacityProperty: true, visibilityProperty: true, contentVisibilityAuto: true })) return false;
    const r = el.getBoundingClientRect();
    for (let e = el; e && e !== document.body; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (s.display === 'none' || s.visibility === 'hidden' || +s.opacity === 0) return false;
      if (e !== el && s.overflow !== 'visible') {
        const q = e.getBoundingClientRect();
        if (q.height < 1 || r.top >= q.bottom - 1 || r.bottom <= q.top + 1) return false;
      }
    }
    return r.width > 0 && r.height > 0;
  };
  const shown = new Map();
  const isShown = (el, stop) => {
    for (let e = el; e && e !== stop.parentElement; e = e.parentElement) {
      if (shown.has(e)) { if (!shown.get(e)) return false; continue; }
      const s = getComputedStyle(e);
      const cls = (e.getAttribute('class') || '') + ' ' + (e.tagName || '');
      const ok = !(s.display === 'none' || s.visibility === 'hidden' || /lineno|line-number|linenumber|gutter|BUTTON|copybutton|md-clipboard/i.test(cls));
      shown.set(e, ok);
      if (!ok) return false;
    }
    return true;
  };
  const blockOf = (el, stop) => {
    for (let e = el; e && e !== stop; e = e.parentElement) {
      const d = getComputedStyle(e).display;
      if (d === 'block' || d === 'flex' || d === 'list-item' || d === 'table-row') return e;
    }
    return stop;
  };
  const visibleText = pre => {
    const ace = pre.querySelectorAll('.ace_text-layer > .ace_line');
    if (ace.length) return [...ace].map(l => l.textContent).join('\n');
    const w = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
    let s = '', prev = null;
    for (let n = w.nextNode(); n; n = w.nextNode()) {
      if (!isShown(n.parentElement, pre)) continue;
      const b = blockOf(n.parentElement, pre);
      if (prev && b !== prev && s && !s.endsWith('\n')) s += '\n';
      prev = b;
      s += n.nodeValue;
    }
    return s;
  };
  const abs = r => [r.left + scrollX, r.top + scrollY, r.right + scrollX, r.bottom + scrollY];
  const R = root.getBoundingClientRect();
  const images = [];
  for (const el of root.querySelectorAll('img, svg, picture, video, canvas, iframe')) {
    if (el.closest('pre, button, nav, .headerlink, .md-annotation, .md-tooltip')) continue;
    if (el.tagName.toLowerCase() === 'svg' && el.parentElement.closest('svg')) continue;
    if (el.tagName.toLowerCase() === 'img' && el.parentElement.closest('picture')) continue;
    if (!vis(el)) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 48 || r.height < 32) continue;
    images.push({ kind: el.tagName.toLowerCase(), src: (el.currentSrc || el.getAttribute('src') || '').split('/').pop(), rect: abs(r) });
  }
  for (const el of root.querySelectorAll('mjx-container[display="true"], .katex-display, .MathJax_Display')) {
    if (!vis(el)) continue;
    const r = el.getBoundingClientRect();
    if (r.width < 24 || r.height < 12) continue;
    const inner = el.querySelector('mjx-math, .katex') || el;
    const q = inner.getBoundingClientRect();
    const m = abs(q.width > 0 ? q : r);
    const k = images.findIndex(o => o.kind === 'math' && Math.min(o.rect[2], m[2]) > Math.max(o.rect[0], m[0]) && Math.min(o.rect[3], m[3]) > Math.max(o.rect[1], m[1]));
    const area = a => (a[2] - a[0]) * (a[3] - a[1]);
    if (k >= 0) { if (area(m) < area(images[k].rect)) images[k].rect = m; continue; }
    images.push({ kind: 'math', src: '', rect: m });
  }
  const codes = [];
  for (const pre of root.querySelectorAll('pre')) {
    if (pre.parentElement.closest('pre') || pre.closest('.linenodiv, .linenos, nav')) continue;
    if (!vis(pre)) continue;
    const r = pre.getBoundingClientRect();
    if (r.height < 8) continue;
    codes.push({ text: visibleText(pre), rect: abs(r) });
  }
  const cut = [];
  if (ylim !== undefined && ylim !== null) {
    for (const el of root.querySelectorAll('p, li, h1, h2, h3, h4, h5, h6, dt, dd, tr, blockquote, figcaption, table, dl, section, div')) {
      if (el.getBoundingClientRect().top + scrollY > ylim) { el.setAttribute('data-hors-capture', ''); cut.push(el); }
    }
  }
  const clone = root.cloneNode(true);
  for (const el of cut) el.removeAttribute('data-hors-capture');
  for (const p of clone.querySelectorAll('[data-hors-capture], pre, script, style, nav, button, mjx-container, .katex, .MathJax')) p.remove();
  document.body.appendChild(clone);
  clone.style.position = 'absolute'; clone.style.left = '-99999px'; clone.style.width = R.width + 'px';
  const text = clone.innerText;
  clone.remove();
  return { root: abs(R), images, codes, text };
}
"""


class _Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a, **k):
        pass


def serve(directory: Path) -> int:
    handler = functools.partial(_Quiet, directory=str(directory))
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv.server_address[1]


def pages_of(site: str, n: int) -> list[str]:
    d = BENCH / "site" / site
    out = []
    for p in sorted(d.rglob("*.html")):
        rel = p.relative_to(d).as_posix()
        if rel.startswith(("404", "print", "toc", "search", "assets/")) or "/assets/" in rel:
            continue
        if site in INCLUDE and not re.match(INCLUDE[site], rel):
            continue
        html = p.read_text(encoding="utf-8", errors="ignore")
        if "<img" in html or "<pre" in html:
            out.append(rel)
    rnd = random.Random(site)
    rnd.shuffle(out)
    return sorted(out[:n])


def capture(site: str, pages: list[str], variants: list[str]) -> list[Path]:
    from playwright.sync_api import sync_playwright

    gen = GEN[SITES[site]]
    port = serve(BENCH / "site" / site)
    cases = []
    with sync_playwright() as p:
        browser = p.chromium.launch()
        for vname in variants:
            scheme, scale = VARIANTS[vname]
            ctx = browser.new_context(viewport=VIEWPORT, device_scale_factor=scale, color_scheme=scheme)
            ctx.route(re.compile(r"^https?://(?!127\.0\.0\.1)"), lambda r: r.abort())
            page = ctx.new_page()
            for rel in pages:
                name = re.sub(r"[^A-Za-z0-9]+", "_", rel.removesuffix(".html").removesuffix("/index"))[:60]
                case = OUT / site / f"{name}-{vname}"
                case.mkdir(parents=True, exist_ok=True)
                try:
                    page.set_viewport_size(VIEWPORT)
                    page.goto(f"http://127.0.0.1:{port}/{rel}", wait_until="load", timeout=30000)
                    page.evaluate("document.fonts.ready")
                    page.evaluate("window.MathJax && MathJax.startup && MathJax.startup.promise ? MathJax.startup.promise.then(() => 1) : 0")
                    page.wait_for_timeout(300)
                    t = page.evaluate(TRUTH, {"sel": gen["content"], "ylim": None})
                    if t:
                        full_h = int(min(t["root"][3] + 40, t["root"][1] + MAX_H + 40))
                        page.set_viewport_size({"width": VIEWPORT["width"], "height": max(VIEWPORT["height"], full_h)})
                        page.wait_for_timeout(300)
                        t = page.evaluate(TRUTH, {"sel": gen["content"], "ylim": max(0, t["root"][1] - 12) + MAX_H})
                except Exception as e:
                    print(f"  {site}/{rel} : {e}", file=sys.stderr)
                    continue
                if not t or not (t["images"] or t["codes"]):
                    continue
                x0, y0, x1, y1 = t["root"]
                x0, y0 = max(0, x0 - 12), max(0, y0 - 12)
                x1 = min(page.evaluate("document.documentElement.scrollWidth"), x1 + 12)
                y1 = min(y1 + 12, y0 + MAX_H)
                clip = {"x": x0, "y": y0, "width": x1 - x0, "height": y1 - y0}
                page.screenshot(path=case / "shot.png", clip=clip)
                def rel_rect(r):
                    return [round((r[0] - x0) * scale), round((r[1] - y0) * scale), round((r[2] - x0) * scale), round((r[3] - y0) * scale)]
                inside = lambda r: r[1] >= y0 and r[3] <= y1
                truth = {
                    "url": rel,
                    "variant": vname,
                    "scale": scale,
                    "images": [dict(i, rect=rel_rect(i["rect"])) for i in t["images"] if inside(i["rect"])],
                    "codes": [dict(c, rect=rel_rect(c["rect"])) for c in t["codes"] if inside(c["rect"])],
                    "text": t["text"],
                    "truncated": t["root"][3] > y1,
                }
                from PIL import Image
                sw, sh = Image.open(case / "shot.png").size
                fits = lambda r: r[0] >= -2 and r[1] >= -2 and r[2] <= sw + 2 and r[3] <= sh + 2
                bad = [o for o in truth["images"] + truth["codes"] if not fits(o["rect"])]
                if bad:
                    print(f"  {site}/{rel} {vname} : {len(bad)} object(s) outside the capture ({sw}x{sh}), dropped", file=sys.stderr)
                    truth["images"] = [o for o in truth["images"] if fits(o["rect"])]
                    truth["codes"] = [o for o in truth["codes"] if fits(o["rect"])]
                (case / "truth.json").write_text(json.dumps(truth, ensure_ascii=False, indent=1), encoding="utf-8")
                cases.append(case)
            ctx.close()
        browser.close()
    return cases


def norm_code(s: str) -> str:
    lines = [l.rstrip() for l in s.replace("\t", "    ").replace(" ", " ").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    common = min((len(l) - len(l.lstrip(" ")) for l in lines if l.strip()), default=0)
    return "\n".join(l[common:] for l in lines)


def visible_images(shot: Path, images: list[dict]) -> list[dict]:
    from PIL import Image
    im = Image.open(shot).convert("RGB")
    w, h = im.size
    px = im.load()
    ring = [px[x, 0] for x in range(0, w, 3)] + [px[x, h - 1] for x in range(0, w, 3)] + \
           [px[0, y] for y in range(0, h, 3)] + [px[w - 1, y] for y in range(0, h, 3)]
    from collections import Counter
    page = Counter(ring).most_common(1)[0][0]
    far = lambda p: sum((a - b) ** 2 for a, b in zip(p, page)) > 10 ** 2
    out = []
    for i in images:
        x0, y0, x1, y1 = [int(round(v)) for v in i["rect"]]
        x0, y0, x1, y1 = max(0, x0), max(0, y0), min(w, x1), min(h, y1)
        if x1 - x0 < 4 or y1 - y0 < 4:
            continue
        cols = [x for x in range(x0, x1) if any(far(px[x, y]) for y in range(y0, y1, 2))]
        rows = [y for y in range(y0, y1) if any(far(px[x, y]) for x in range(x0, x1, 2))]
        if not cols or not rows:
            continue
        r = [cols[0], rows[0], cols[-1] + 1, rows[-1] + 1]
        if (r[2] - r[0]) * (r[3] - r[1]) < 0.02 * (x1 - x0) * (y1 - y0):
            continue
        sample = [px[x, y] for x in range(r[0], r[2], max(1, (r[2] - r[0]) // 24)) for y in range(r[1], r[3], max(1, (r[3] - r[1]) // 24))]
        if len(set(sample)) <= 2:
            continue
        out.append(dict(i, visible=r))
    return out


def run_case(case: Path, exe: Path, rescore: bool = False) -> dict:
    env = dict(os.environ, MARKDOWNSHOT_MODELS=str(ROOT / "models"))
    tag = "" if exe == EXE else "-" + exe.name
    fig = case / f"fig{tag}.json"
    if rescore and fig.is_file() and (case / f"got{tag}.md").is_file():
        md = (case / f"got{tag}.md").read_text(encoding="utf-8")
    else:
        p = subprocess.run([str(exe), "convert", str(case / "shot.png"), "--stdout", "--figures", str(fig)],
                           capture_output=True, text=True, env=env)
        if p.returncode != 0:
            return {"case": case.name, "error": p.stderr[-400:]}
        md = p.stdout
        (case / f"got{tag}.md").write_text(md, encoding="utf-8")
        m = re.search(r"\((\d+) ms : OCR (\d+) ms", p.stderr)
        if m:
            (case / f"ms{tag}.txt").write_text(f"{m.group(1)} {m.group(2)}", encoding="utf-8")
    t = json.loads((case / "truth.json").read_text(encoding="utf-8"))
    t["images"] = [i for i in t["images"] if i["kind"] != "iframe"]
    t["images"] = visible_images(case / "shot.png", t["images"])
    got_figs = ev.load_figures(fig)
    exp = []
    for e in t["images"]:
        best = max(got_figs, key=lambda g: max(ev.iou(e["rect"], g["rect"]), ev.iou(e["visible"], g["rect"])), default=None)
        if best is not None and ev.iou(e["visible"], best["rect"]) > ev.iou(e["rect"], best["rect"]):
            exp.append(dict(e, rect=e["visible"]))
        else:
            exp.append(e)
    pairs, missed, extra = ev.match_figures(exp, got_figs, 0.5)
    ious = [v for _, _, v in pairs]
    ce = [norm_code(c["text"]) for c in t["codes"]]
    cg = [norm_code(c) for c in ev.code_blocks(md)]
    cpairs, cextra = ev.match_code(ce, cg)
    exact = sum(1 for k, j, _ in cpairs if j >= 0 and ce[k] == cg[j])
    errs = sum(ev.lev(ce[k], cg[j]) if j >= 0 else len(ce[k]) for k, j, _ in cpairs)
    chars = sum(len(c) for c in ce)
    we = ev.words(t["text"])
    wg = ev.words(re.sub(r"^(`{3,}).*?^\1(?!`)", " ", md, flags=re.S | re.M))
    from collections import Counter
    inter = sum((Counter(we) & Counter(wg)).values())
    f1 = 2 * inter / max(1, len(we) + len(wg))
    msf = case / f"ms{tag}.txt"
    ms, ocr_ms = (int(v) for v in msf.read_text().split()) if msf.is_file() else (0, 0)
    return {
        "case": f"{case.parent.name}/{case.name}",
        "ms": ms,
        "ocr_ms": ocr_ms,
        "img": len(t["images"]),
        "img_exact": sum(1 for v in ious if v >= 0.95),
        "img_ok": sum(1 for v in ious if v >= 0.80),
        "img_missed": len(missed),
        "img_extra": len(extra),
        "img_detail": {"pairs": pairs, "missed": missed, "extra": extra},
        "code": len(ce),
        "code_exact": exact,
        "code_missed": sum(1 for _, j, _ in cpairs if j < 0),
        "code_extra": len(cextra),
        "code_chars": chars,
        "code_errs": errs,
        "f1": f1,
    }


def summarize(rows: list[dict]) -> dict:
    s = {k: sum(r.get(k, 0) for r in rows) for k in
         ["img", "img_exact", "img_ok", "img_missed", "img_extra", "code", "code_exact", "code_missed", "code_extra", "code_chars", "code_errs"]}
    s["pages"] = len(rows)
    s["ms"] = sum(r.get("ms", 0) for r in rows)
    s["f1"] = sum(r["f1"] for r in rows) / max(1, len(rows))
    return s


def line(name: str, s: dict, base: dict | None = None) -> str:
    def pct(a, b):
        return f"{100 * a / b:.1f} %" if b else "n/a"
    cells = [
        name, str(s["pages"]),
        f"{s['img']}", pct(s["img_exact"], s["img"]), pct(s["img_ok"], s["img"]), str(s["img_missed"]), str(s["img_extra"]),
        f"{s['code']}", pct(s["code_exact"], s["code"]), str(s["code_missed"]), f"{100 * s['code_errs'] / max(1, s['code_chars']):.2f} %",
        f"{s['f1']:.3f}",
        f"{s['ms'] / max(1, s['pages']) / 1000:.2f} s",
    ]
    if base:
        cells[3] += f" (avant {pct(base['img_exact'], base['img'])})"
        cells[8] += f" (avant {pct(base['code_exact'], base['code'])})"
    return "| " + " | ".join(cells) + " |"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sites", default=",".join(SITES))
    ap.add_argument("--pages", type=int, default=12, help="pages sampled per site")
    ap.add_argument("--variants", default="light-100,dark-125")
    ap.add_argument("--no-capture", action="store_true", help="reuse existing captures")
    ap.add_argument("--rescore", action="store_true", help="also reuse engine outputs (scoring only)")
    ap.add_argument("--exe", type=Path, default=EXE)
    ap.add_argument("--save")
    ap.add_argument("--compare")
    ap.add_argument("-j", type=int, default=3)
    a = ap.parse_args()
    variants = a.variants.split(",")
    all_rows = []
    per_site = {}
    for site in a.sites.split(","):
        if a.no_capture:
            cases = sorted(c for c in (OUT / site).iterdir() if (c / "truth.json").is_file() and c.name.rsplit("-", 2)[-2] + "-" + c.name.rsplit("-", 1)[-1] in variants)
        else:
            cases = capture(site, pages_of(site, a.pages), variants)
        with ThreadPoolExecutor(a.j) as pool:
            rows = list(pool.map(lambda c: run_case(c, a.exe, a.rescore), cases))
        errs = [r for r in rows if "error" in r]
        for r in errs:
            print(f"  error {r['case']}: {r['error']}", file=sys.stderr)
        rows = [r for r in rows if "error" not in r]
        per_site[site] = rows
        all_rows += rows
    base = json.load(open(a.compare)) if a.compare else None
    print("| site | pages | images | exact frame | correct | missed | extra | code blocks | identical | missed | code CER | text F1 | time / page |")
    print("|---" * 13 + "|")
    for site, rows in per_site.items():
        b = summarize([r for r in base["rows"] if r["case"].startswith(site + "/")]) if base else None
        print(line(site, summarize(rows), b))
    b = summarize([r for r in base["rows"] if any(r["case"].startswith(s + "/") for s in per_site)]) if base else None
    print(line("**total**", summarize(all_rows), b))
    if a.save:
        json.dump({"rows": all_rows}, open(a.save, "w"), indent=1)


if __name__ == "__main__":
    main()
