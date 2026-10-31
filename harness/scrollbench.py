#!/usr/bin/env python3
import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "harness"))
import realbench as rb

OUT = ROOT / "target" / "scrollbench"
EXE = ROOT / "target" / "release" / "markdownshot"
DISPLAY = os.environ.get("SCROLLBENCH_DISPLAY", ":91")
WIN = (1280, 900)
MAX_H = 9000


def ensure_xvfb():
    n = DISPLAY.lstrip(":").split(".")[0]
    if not Path(f"/tmp/.X11-unix/X{n}").exists():
        subprocess.Popen(["Xvfb", DISPLAY, "-screen", "0", "1400x1000x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.0)


def row_keys(img: np.ndarray):
    q = (img >> 2).astype(np.uint8)
    keys = []
    for y in range(q.shape[0]):
        r = q[y]
        if int(r.max()) - int(r.min()) < 3:
            keys.append(None)
        else:
            keys.append(hash(r.tobytes()))
    return keys


def score(got: np.ndarray, truth: np.ndarray, cols=None, dynamic=None) -> dict:
    w = min(got.shape[1], truth.shape[1])
    x0, x1 = cols if cols else (0, w)
    x0, x1 = max(0, x0), min(w, x1)
    got, truth = got[:, x0:x1].astype(np.int16), truth[:, x0:x1].astype(np.int16)
    n = min(got.shape[0], truth.shape[0])
    content = (truth.max(axis=(1, 2)) - truth.min(axis=(1, 2))) > 12
    if dynamic is not None:
        dyn = np.zeros(truth.shape[0], bool)
        dyn[: len(dynamic)] = dynamic[: truth.shape[0]]
        content &= ~dyn
    diff = (np.abs(got[:n] - truth[:n]).max(axis=2) > 24).mean(axis=1)
    same = np.zeros(truth.shape[0], bool)
    same[:n] = diff < 0.005
    kg, kt = row_keys(got.astype(np.uint8)), row_keys(truth.astype(np.uint8))
    set_g = {k for k in kg if k is not None}
    found = np.array([same[y] or (k is not None and k in set_g) for y, k in enumerate(kt)])
    pos = {}
    for y, k in enumerate(kt):
        if k is not None:
            pos.setdefault(k, []).append(y)
    shifts, prev = 0, 0
    for y, k in enumerate(kg):
        if k is not None and k in pos and len(pos[k]) == 1:
            o = pos[k][0] - y
            if o != prev and abs(o - prev) > 2:
                shifts += 1
            prev = o
    nc = max(1, int(content.sum()))
    return {
        "ok": float((same & content).sum() / nc),
        "lost": float((~found & content).sum() / nc),
        "shifts": shifts,
        "h_ratio": got.shape[0] / truth.shape[0],
        "bad_rows": [int(y) for y in np.nonzero(content & ~same)[0][:400]],
    }


def wheel(n: int):
    subprocess.run(["xdotool", "click", "--repeat", str(n), "--delay", "1", "5"], env={**os.environ, "DISPLAY": DISPLAY})


def dynamic_rows(a: np.ndarray, b: np.ndarray, cols) -> np.ndarray:
    n = min(a.shape[0], b.shape[0])
    x0, x1 = cols if cols else (0, a.shape[1])
    d = (np.abs(a[:n, x0:x1].astype(np.int16) - b[:n, x0:x1].astype(np.int16)).max(axis=2) > 24).any(axis=1)
    out = d.copy()
    for k in range(1, 9):
        out[k:] |= d[:-k]
        out[:-k] |= d[k:]
    return out


def run_case(page, zone, cfg: str, case: Path, truth: np.ndarray, cols, dynamic=None) -> dict:
    x, y, w, h = zone
    if page.evaluate("scrollY") != 0:
        page.reload(wait_until="load")
        page.evaluate("document.fonts.ready")
        page.evaluate("window.scrollTo({top: 0, behavior: 'instant'})")
        page.wait_for_timeout(700)
    frames = case / f"frames-{cfg.replace(':', '_')}"
    if frames.exists():
        for f in frames.iterdir():
            f.unlink()
    env = {**os.environ, "DISPLAY": DISPLAY, "MARKDOWNSHOT_FRAMES": str(frames)}
    out = case / f"got-{cfg.replace(':', '_')}.png"
    args = [str(EXE), "record", "--zone", f"{x},{y},{w},{h}", "--no-note", "--image", str(out)]
    kind, *par = cfg.split(":")
    if kind == "auto":
        args += ["--auto", "--step", par[0], "--duration", "120"]
    else:
        args += ["--fps", par[1], "--duration", "120"]
    t0 = time.time()
    proc = subprocess.Popen(args, env=env, stdin=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if kind == "main":
        rate = float(par[0])
        time.sleep(0.6)
        subprocess.run(["xdotool", "mousemove", str(x + w // 2), str(y + h // 2)], env=env)
        last, same = -1, 0
        while time.time() - t0 < 110:
            wheel(1)
            time.sleep(1.0 / rate)
            pos = page.evaluate("scrollY")
            if pos == last:
                same += 1
                if same > rate * 0.5:
                    break
            else:
                same = 0
            last = pos
            if pos > MAX_H - h:
                break
        time.sleep(0.8)
        proc.stdin.write("\n")
        proc.stdin.flush()
    _, err = proc.communicate(timeout=180)
    dt = time.time() - t0
    m = re.search(r"\(recording (\d+) ms: (\d+) of (\d+) frames, (\d+) gap", err)
    if not out.exists():
        return {"cfg": cfg, "error": err.strip()[-300:]}
    got = np.asarray(Image.open(out).convert("RGB"))
    th = min(truth.shape[0], got.shape[0] + h) if kind == "main" else truth.shape[0]
    r = score(got[:MAX_H], truth[: min(th, MAX_H)], cols, dynamic)
    (case / f"bad-{cfg.replace(':', '_')}.json").write_text(json.dumps({"cols": cols, "bad_rows": r.pop("bad_rows")}))
    r.update({
        "cfg": cfg,
        "secs": round(dt, 2),
        "frames": int(m.group(2)) if m else None,
        "gaps": int(m.group(4)) if m else None,
        "px_s": round(got.shape[0] / dt),
    })
    return r


def replay(only: str | None):
    exe = ROOT / "target" / "release" / "examples" / "stitch"
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "markdownshot-core", "--example", "stitch"], check=True)
    rows = []
    for meta in sorted(OUT.glob("*/*/bad-*.json")):
        case = meta.parent
        cfg = meta.stem.removeprefix("bad-")
        if only and only not in str(case):
            continue
        frames = case / f"frames-{cfg}"
        if not frames.exists():
            continue
        out = case / f"rejoue-{cfg}.png"
        t = time.time()
        p = subprocess.run([str(exe), str(frames), str(out)], capture_output=True, text=True)
        dt = time.time() - t
        truth = np.asarray(Image.open(case / "truth.png").convert("RGB"))
        got = np.asarray(Image.open(out).convert("RGB"))
        cols = json.loads(meta.read_text())["cols"]
        dyn = np.load(case / "dynamic.npy") if (case / "dynamic.npy").exists() else None
        r = score(got[:MAX_H], truth[:MAX_H], cols, dyn)
        r.pop("bad_rows")
        gaps = re.search(r"(\d+) gaps", p.stdout)
        rows.append(r)
        print(
            f"{case.parent.name}/{case.name[:40]:40} {cfg:10} ok {r['ok']:.4f} lost {r['lost']:.4f} shifts {r['shifts']} "
            f"height {r['h_ratio']:.3f} gaps {gaps.group(1) if gaps else '?'}  {dt:.1f} s",
            flush=True,
        )
    if rows:
        clean = sum(1 for r in rows if r["ok"] >= 0.995 and r["lost"] < 0.002)
        print(f"\nreplayed: {len(rows)} cases, lossless {clean}, ok {sum(r['ok'] for r in rows) / len(rows):.4f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sites", default="rust-book,godot-docs,mkdocs-material,rn-website")
    ap.add_argument("--pages", type=int, default=3)
    ap.add_argument("--configs", default="auto:0.75")
    ap.add_argument("--smooth", default="on", choices=["on", "off"])
    ap.add_argument("--save")
    ap.add_argument("--only", help="only pages whose path contains this text")
    ap.add_argument("--replay", action="store_true", help="re-stitch the recorded frames, without a browser")
    a = ap.parse_args()
    if a.replay:
        replay(a.only)
        return
    ensure_xvfb()
    os.environ["DISPLAY"] = DISPLAY
    from playwright.sync_api import sync_playwright

    results = []
    with sync_playwright() as p:
        flags = ["--window-position=0,0", f"--window-size={WIN[0]},{WIN[1]}", "--hide-scrollbars"]
        flags.append("--enable-smooth-scrolling" if a.smooth == "on" else "--disable-smooth-scrolling")
        browser = p.chromium.launch(headless=False, args=flags)
        for site in a.sites.split(","):
            port = rb.serve(rb.BENCH / "site" / site)
            ctx = browser.new_context(no_viewport=True)
            ctx.route(re.compile(r"^https?://(?!127\.0\.0\.1)"), lambda r: r.abort())
            page = ctx.new_page()
            for rel in rb.pages_of(site, a.pages):
                if a.only and a.only not in rel:
                    continue
                name = re.sub(r"[^A-Za-z0-9]+", "_", rel.removesuffix(".html"))[:60]
                case = OUT / site / name
                case.mkdir(parents=True, exist_ok=True)
                page.goto(f"http://127.0.0.1:{port}/{rel}", wait_until="load", timeout=30000)
                page.evaluate("document.fonts.ready")
                page.wait_for_timeout(600)
                g = page.evaluate("[screenX, screenY, outerHeight - innerHeight, innerWidth, innerHeight, document.documentElement.clientWidth]")
                zone = (g[0], g[1] + g[2], g[5], g[4])
                sel = rb.GEN[rb.SITES[site]]["content"]
                cols = page.evaluate(
                    "(sel) => { const e = [...document.querySelectorAll(sel)].sort((a, b) => b.getBoundingClientRect().height - a.getBoundingClientRect().height)[0];"
                    " if (!e) return null; const r = e.getBoundingClientRect(); return [Math.floor(r.left), Math.ceil(r.right)]; }",
                    sel,
                )
                tpath = case / "truth.png"
                page.screenshot(path=str(tpath), full_page=True)
                page.wait_for_timeout(700)
                page.screenshot(path=str(case / "truth2.png"), full_page=True)
                truth = np.asarray(Image.open(tpath).convert("RGB"))
                dynamic = dynamic_rows(truth, np.asarray(Image.open(case / "truth2.png").convert("RGB")), cols)
                np.save(case / "dynamic.npy", dynamic)
                page.reload(wait_until="load")
                page.evaluate("document.fonts.ready")
                page.evaluate("window.scrollTo({top: 0, behavior: 'instant'})")
                page.wait_for_timeout(600)
                for cfg in a.configs.split(","):
                    r = run_case(page, zone, cfg, case, truth, cols, dynamic)
                    r.update({"site": site, "page": rel, "truth_h": int(truth.shape[0])})
                    results.append(r)
                    if "error" in r:
                        print(f"{site}/{rel} {cfg}: ERROR {r['error']}", flush=True)
                    else:
                        print(
                            f"{site}/{name[:40]:40} {cfg:12} ok {r['ok']:.4f} lost {r['lost']:.4f} "
                            f"shifts {r['shifts']} height {r['h_ratio']:.3f} {r['secs']:6.1f} s {r['px_s']:5} px/s "
                            f"{r['frames']} frames {r['gaps']} gaps",
                            flush=True,
                        )
            ctx.close()
        browser.close()
    print()
    for cfg in a.configs.split(","):
        rs = [r for r in results if r.get("cfg") == cfg and "error" not in r]
        if not rs:
            continue
        mean = lambda k: sum(r[k] for r in rs) / len(rs)
        clean = sum(1 for r in rs if r["ok"] >= 0.995 and r["lost"] < 0.002)
        print(
            f"{cfg:12} pages {len(rs):3}  lossless {clean}/{len(rs)}  ok {mean('ok'):.4f}  lost {mean('lost'):.4f}  "
            f"shifts {sum(r['shifts'] for r in rs)}  {sum(r['secs'] for r in rs) / len(rs):.1f} s/page  {mean('px_s'):.0f} px/s"
        )
    if a.save:
        Path(a.save).write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
