#!/usr/bin/env python3
import argparse
import os
import random
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "testdata" / "calibration" / "out"
EXE = ROOT / "target" / "release" / ("markdownshot.exe" if os.name == "nt" else "markdownshot")


def convert(png: Path, exe: Path = EXE) -> str:
    env = dict(os.environ, MARKDOWNSHOT_MODELS=str(ROOT / "models"))
    p = subprocess.run([str(exe), "convert", str(png), "--stdout"], capture_output=True, text=True, env=env)
    if p.returncode != 0:
        raise RuntimeError(p.stderr)
    return p.stdout


def plain(block: str) -> str:
    s = re.sub(r"^(#{1,6} |> |- |\d+\. )", "", block.strip())
    s = re.sub(r"[`*]", "", s)
    return re.sub(r"\s+", " ", s).strip()


def blocks(md: str) -> list[str]:
    out = []
    for b in re.split(r"\n\s*\n", md):
        b = b.strip()
        if not b or b.startswith("```") or b.startswith("|") or b.startswith("![["):
            continue
        if re.match(r"^(\s*)(- |\d+\. )", b):
            out.extend(line.strip() for line in b.splitlines() if line.strip())
        else:
            out.append(re.sub(r"\s*\n\s*", " ", b))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=6, help="areas per page")
    ap.add_argument("--only", default="")
    ap.add_argument("--show", action="store_true")
    ap.add_argument("--exe", type=Path, default=EXE, help="markdownshot executable (e.g. an older build)")
    a = ap.parse_args()
    dirs = sorted(d for d in OUT.iterdir() if (d / "full.png").is_file())
    if a.only:
        dirs = [d for d in dirs if any(o in d.name for o in a.only.split(","))]
    tot_cmp = tot_bad = 0
    print("| page | blocks compared | different formatting |")
    print("|---|---|---|")
    for d in dirs:
        full_md = convert(d / "full.png", a.exe)
        ref = {}
        for b in blocks(full_md):
            ref.setdefault(plain(b), b)
        img = Image.open(d / "full.png")
        w, h = img.size
        rnd = random.Random(d.name)
        n_cmp = n_bad = 0
        with tempfile.TemporaryDirectory() as tmp:
            for k in range(a.n):
                ch = rnd.randint(int(h * 0.08), int(h * 0.25))
                y0 = rnd.randint(0, h - ch)
                x0 = rnd.randint(int(w * 0.10), int(w * 0.20))
                x1 = w - rnd.randint(int(w * 0.10), int(w * 0.20))
                png = Path(tmp) / f"{k}.png"
                img.crop((x0, y0, x1, y0 + ch)).save(png)
                for b in blocks(convert(png, a.exe)):
                    r = ref.get(plain(b))
                    if r is None:
                        continue
                    n_cmp += 1
                    if r.strip() != b.strip():
                        n_bad += 1
                        if a.show:
                            print(f"  {d.name} area {k} ({x0},{y0},{x1},{y0 + ch}):\n    page: {r[:160]}\n    area: {b[:160]}")
        tot_cmp += n_cmp
        tot_bad += n_bad
        print(f"| {d.name} | {n_cmp} | {n_bad} |")
    print(f"| **total** | {tot_cmp} | {tot_bad} ({100 * tot_bad / max(1, tot_cmp):.1f} %) |")


if __name__ == "__main__":
    main()
