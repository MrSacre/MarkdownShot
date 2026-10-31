#!/usr/bin/env python3
import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "harness"))
import eval as ev
import realbench as rb


def align(a: str, b: str):
    n, m = len(a), len(b)
    d = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1):
        d[i][0] = i
    for j in range(m + 1):
        d[0][j] = j
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            d[i][j] = min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (a[i - 1] != b[j - 1]))
    ops = []
    i, j = n, m
    while i > 0 or j > 0:
        if i > 0 and j > 0 and d[i][j] == d[i - 1][j - 1] + (a[i - 1] != b[j - 1]):
            if a[i - 1] != b[j - 1]:
                ops.append(("sub", a[i - 1], b[j - 1]))
            i, j = i - 1, j - 1
        elif i > 0 and d[i][j] == d[i - 1][j] + 1:
            ops.append(("del", a[i - 1], ""))
            i -= 1
        else:
            ops.append(("ins", "", b[j - 1]))
            j -= 1
    return ops


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag", default="markdownshot-r9")
    ap.add_argument("--top", type=int, default=40)
    ap.add_argument("--examples")
    a = ap.parse_args()
    conf = Counter()
    missed_chars = matched_errs = chars = 0
    per_block = []
    for case in sorted(rb.OUT.glob("*/*")):
        got = case / f"got-{a.tag}.md"
        if not got.is_file() or not (case / "truth.json").is_file():
            continue
        t = json.loads((case / "truth.json").read_text(encoding="utf-8"))
        md = got.read_text(encoding="utf-8")
        ce = [rb.norm_code(c["text"]) for c in t["codes"]]
        cg = [rb.norm_code(c) for c in ev.code_blocks(md)]
        pairs, _ = ev.match_code(ce, cg)
        for k, j, _ in pairs:
            chars += len(ce[k])
            if j < 0:
                missed_chars += len(ce[k])
                continue
            if ce[k] == cg[j]:
                continue
            if len(ce[k]) * len(cg[j]) > 4_000_000:
                matched_errs += ev.lev(ce[k], cg[j])
                continue
            ops = align(ce[k], cg[j])
            matched_errs += len(ops)
            per_block.append((len(ops), f"{case.parent.name}/{case.name}", k))
            for op in ops:
                conf[op] += 1
                if a.examples and a.examples in repr(op):
                    print(case, k)
    print(f"code characters: {chars}, errors in missed blocks: {missed_chars} ({missed_chars / chars:.1%}), "
          f"errors in found blocks: {matched_errs} ({matched_errs / chars:.1%})")
    print("\nmost frequent confusions (op, expected, got):")
    for (op, e, g), n in conf.most_common(a.top):
        print(f"  {n:6}  {op:3}  {e!r:8} -> {g!r}")
    print("\nmost damaged blocks:")
    for n, c, k in sorted(per_block, reverse=True)[:15]:
        print(f"  {n:5}  {c}  block {k}")


if __name__ == "__main__":
    main()
