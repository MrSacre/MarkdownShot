#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "harness"))
import eval as ev

OUT = ROOT / "testdata" / "calibration" / "out"
EXE = ROOT / "target" / "release" / ("markdownshot.exe" if os.name == "nt" else "markdownshot")
COLS = [("score", "composite"), ("f1", "word F1"), ("cer_text", "text CER"), ("cer_code", "code CER"),
        ("structure", "structure"), ("fig_score", "framing"), ("fig_iou", "mean IoU"),
        ("fig_extra", "extra img"), ("fig_missed", "missed img"), ("tables", "cells")]


def run_page(d: Path, source: str, work: Path, exe: Path = EXE):
    work.mkdir(parents=True, exist_ok=True)
    fig = work / "fig.json"
    md = work / "got.md"
    if source == "full":
        cmd = [str(exe), "convert", str(d / "full.png"), "--stdout", "--figures", str(fig)]
    else:
        cmd = [str(exe), "stitch", str(d / source), "--stdout", "--figures", str(fig)]
    env = dict(os.environ, MARKDOWNSHOT_MODELS=str(ROOT / "models"))
    p = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if p.returncode != 0:
        raise RuntimeError(f"{d.name} : {p.stderr}")
    md.write_text(p.stdout, encoding="utf-8")
    r = ev.evaluate((d / "expected.md").read_text(encoding="utf-8"), p.stdout,
                    ev.load_figures(fig), ev.load_figures(d / "figures.json"))
    blocks = r.pop("blocks")
    f = r["fig"]
    row = {"score": r["score"], "f1": r["f1"], "cer_text": r["cer_text"], "cer_code": r["cer_code"],
           "structure": r["structure"], "fig_score": f["score"], "fig_iou": f["iou_mean"],
           "fig_extra": len(f["extra"]), "fig_missed": len(f["missed"]), "tables": r["tables"][0]}
    return d.name, row, r, blocks


def fmt(k, v):
    return f"{v}" if isinstance(v, int) else f"{v:.3f}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default="frames", choices=["frames", "frames_video", "full"])
    ap.add_argument("--only", default="")
    ap.add_argument("--dir", type=Path, default=OUT,
                    help="cases folder (one subfolder per case: expected.md, figures.json, frames/...), "
                         "e.g. target/real for unversioned real cases")
    ap.add_argument("--save")
    ap.add_argument("--compare")
    ap.add_argument("--show", action="store_true", help="detailed report for each page")
    ap.add_argument("-j", type=int, default=2, help="pages processed in parallel")
    ap.add_argument("--exe", type=Path, default=EXE, help="markdownshot executable (e.g. an older build)")
    a = ap.parse_args()
    dirs = sorted(d for d in a.dir.iterdir() if (d / "expected.md").is_file())
    if a.only:
        dirs = [d for d in dirs if any(o in d.name for o in a.only.split(","))]
    work = ROOT / "target" / "calibration" / (a.source if a.exe == EXE else f"{a.source}-{a.exe.parent.parent.name}")
    with ThreadPoolExecutor(a.j) as pool:
        res = list(pool.map(lambda d: run_page(d, a.source, work / d.name, a.exe), dirs))
    rows = {n: row for n, row, _, _ in res}
    if a.show:
        for n, _, r, blocks in res:
            print(f"\n=================== {n}")
            r["blocks"] = blocks
            ev.report(r)
    base = json.load(open(a.compare)) if a.compare else None
    print()
    print("| page | " + " | ".join(lab for _, lab in COLS) + " |")
    print("|---" * (len(COLS) + 1) + "|")
    for n, row in rows.items():
        cells = []
        for k, _ in COLS:
            v = row[k]
            if base and n in base and k in base[n]:
                b = base[n][k]
                cells.append(f"{fmt(k, b)} → **{fmt(k, v)}**" if fmt(k, b) != fmt(k, v) else fmt(k, v))
            else:
                cells.append(fmt(k, v))
        print(f"| {n} | " + " | ".join(cells) + " |")
    mean = {k: sum(r[k] for r in rows.values()) / len(rows) for k, _ in COLS}
    line = []
    for k, _ in COLS:
        if base and any(k in base[n] for n in rows if n in base):
            bm = sum(base[n][k] for n in rows if n in base and k in base[n]) / max(1, sum(1 for n in rows if n in base and k in base[n]))
            line.append(f"{bm:.3f} → **{mean[k]:.3f}**")
        else:
            line.append(f"{mean[k]:.3f}")
    print("| **mean** | " + " | ".join(line) + " |")
    if a.save:
        json.dump(rows, open(a.save, "w"), indent=1)


if __name__ == "__main__":
    main()
