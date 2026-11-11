#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "harness"))
import eval as ev
import realbench as rb

OUT = ROOT / "target" / "blockbench"


def build(stride: int, max_lines: int) -> int:
    from PIL import Image

    n = 0
    for site in sorted(p for p in rb.OUT.iterdir() if p.is_dir()):
        cases = sorted(c for c in site.iterdir() if (c / "truth.json").is_file())
        for case in cases:
            t = json.loads((case / "truth.json").read_text(encoding="utf-8"))
            page = case.name.rsplit("-" + t["variant"], 1)[0]
            pages = sorted({c.name.rsplit("-" + json.loads((c / "truth.json").read_text(encoding="utf-8"))["variant"], 1)[0] for c in cases})
            if pages.index(page) % stride:
                continue
            im = Image.open(case / "shot.png").convert("RGB")
            for k, c in enumerate(t["codes"]):
                text = rb.norm_code(c["text"])
                if not text.strip() or text.count("\n") + 1 > max_lines:
                    continue
                x0, y0, x1, y1 = c["rect"]
                box = (0, max(0, y0 - 160), im.width, min(im.height, y1 + 160))
                d = OUT / site.name
                d.mkdir(parents=True, exist_ok=True)
                stem = f"{case.name}-{k:02d}"
                im.crop(box).save(d / f"{stem}.png")
                (d / f"{stem}.json").write_text(json.dumps({"text": text, "variant": t["variant"], "fonts": t.get("fonts")}, ensure_ascii=False), encoding="utf-8")
                n += 1
    return n


def run_one(png: Path, exe: Path, tag: str, reuse: bool) -> dict:
    out = png.with_suffix(f".{tag}.md")
    if not (reuse and out.is_file()):
        env = dict(os.environ, MARKDOWNSHOT_MODELS=str(ROOT / "models"))
        p = subprocess.run([str(exe), "convert", str(png), "--stdout"], capture_output=True, text=True, env=env)
        out.write_text(p.stdout, encoding="utf-8")
    md = out.read_text(encoding="utf-8")
    t = json.loads(png.with_suffix(".json").read_text(encoding="utf-8"))
    exp = t["text"]
    blocks = [rb.norm_code(c) for c in ev.code_blocks(md)]
    got = max(blocks, key=lambda b: ev.ratio(exp, b), default=None)
    found = got is not None and ev.ratio(exp, got) >= 0.3
    ops = rb.diff_ops(exp, got) if found else [("del", c, "") for c in exp]
    sym = [f"{e or '∅'}→{g or '∅'}" for _, e, g in ops if e in rb.SYMBOLS or g in rb.SYMBOLS]
    return {
        "case": f"{png.parent.name}/{png.stem}",
        "variant": t["variant"],
        "chars": len(exp),
        "errs": len(ops),
        "found": found,
        "exact": found and got == exp,
        "sym_chars": sum(1 for c in exp if c in rb.SYMBOLS),
        "sym_errs": len(sym),
        "conf": [f"{e or '∅'}→{g or '∅'}" for _, e, g in ops],
    }


def table(rows: list[dict], base: list[dict] | None) -> None:
    def line(name, rs, bs):
        def cell(f):
            v = f(rs)
            return f"{v}" + (f" ({f(bs)})" if bs else "")
        n = len(rs)
        cols = [
            name, str(n),
            cell(lambda r: f"{100 * sum(x['exact'] for x in r) / max(1, len(r)):.1f} %"),
            cell(lambda r: str(sum(not x["found"] for x in r))),
            cell(lambda r: f"{100 * sum(x['errs'] for x in r) / max(1, sum(x['chars'] for x in r)):.2f} %"),
            cell(lambda r: f"{100 * sum(x['sym_errs'] for x in r) / max(1, sum(x['sym_chars'] for x in r)):.2f} %"),
            cell(lambda r: f"{100 * sum(x['errs'] for x in r if x['found']) / max(1, sum(x['chars'] for x in r if x['found'])):.2f} %"),
        ]
        print("| " + " | ".join(cols) + " |")
    print("| variant | blocks | identical | not found | CER | symbol errors | CER of found blocks |")
    print("|---|---|---|---|---|---|---|")
    for v in sorted({r["variant"] for r in rows}):
        line(v, [r for r in rows if r["variant"] == v], [r for r in base if r["variant"] == v] if base else None)
    line("**total**", rows, base)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--build", action="store_true", help="crop the code blocks of the realbench captures")
    ap.add_argument("--stride", type=int, default=2)
    ap.add_argument("--max-lines", type=int, default=60)
    ap.add_argument("--exe", type=Path, default=rb.EXE)
    ap.add_argument("--tag", default="got")
    ap.add_argument("--reuse", action="store_true")
    ap.add_argument("--every", type=int, default=1, help="evaluate one block out of N")
    ap.add_argument("--save")
    ap.add_argument("--compare")
    ap.add_argument("--top", type=int, default=0, help="print the N most frequent confusions")
    ap.add_argument("-j", type=int, default=2)
    a = ap.parse_args()
    if a.build:
        print(f"{build(a.stride, a.max_lines)} blocks")
        return
    pngs = sorted(OUT.glob("*/*.png"))[:: a.every]
    with ThreadPoolExecutor(a.j) as pool:
        rows = list(pool.map(lambda p: run_one(p, a.exe, a.tag, a.reuse), pngs))
    base = json.load(open(a.compare))["rows"] if a.compare else None
    table(rows, base)
    if a.top:
        c = Counter(x for r in rows for x in r["conf"])
        print(sum(c.values()), c.most_common(a.top))
    if a.save:
        json.dump({"rows": rows}, open(a.save, "w"))


if __name__ == "__main__":
    main()
