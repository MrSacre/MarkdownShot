#!/usr/bin/env python3
import argparse
import json
import re
import sys
import unicodedata
from difflib import SequenceMatcher


def norm(s):
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    return s.lower()


def strip_yaml(md):
    lines = md.split("\n")
    if lines and lines[0].strip() == "---":
        j = 1
        while j < len(lines) and lines[j].strip() != "---":
            j += 1
        return "\n".join(lines[j + 1:])
    return md


def words(md):
    md = re.sub(r"!\[\[.*?\]\]|!\[.*?\]\(.*?\)", " ", md)
    md = re.sub(r"^(`{3,}).*?^\1(?!`)", " ", md, flags=re.S | re.M)
    md = re.sub(r"[#>*`|_\[\]()-]", " ", md)
    return re.findall(r"[0-9a-z]+", norm(md))


def code_blocks(md):
    return [b.rstrip("\n") for _, b in re.findall(r"^(`{3,})[a-z]*\n(.*?)^\1(?!`)", md, flags=re.S | re.M)]


def prose(md):
    md = re.sub(r"^(`{3,}).*?^\1(?!`)", " ", md, flags=re.S | re.M)
    md = re.sub(r"!\[\[.*?\]\]|!\[.*?\]\(.*?\)", " ", md)
    md = re.sub(r"^\s*(#{1,6} |> |[-*] |\d+[.)] )", "", md, flags=re.M)
    md = re.sub(r"^\s*#{1,6} ", "", md, flags=re.M)
    md = md.replace("**", "").replace("`", "").replace("\\", "")
    return " ".join(md.split())


def blocks(md):
    out = []
    lines = strip_yaml(md).split("\n")
    i = 0
    indents = []
    while i < len(lines):
        ln = lines[i]
        if ln.strip().startswith("```"):
            fence = re.match(r"`+", ln.strip()).group(0)
            j = i + 1
            while j < len(lines) and lines[j].strip() != fence:
                j += 1
            out.append(("code", "\n".join(lines[i + 1:j])))
            indents = []
            i = j + 1
            continue
        m_li = re.match(r"(\s*)([-*]|\d+[.)]) (.*)", ln)
        if m_li:
            ind = len(m_li.group(1))
            while indents and indents[-1] > ind:
                indents.pop()
            if not indents or indents[-1] < ind:
                indents.append(ind)
            lvl = len(indents) - 1
            kind = "oli" if m_li.group(2)[0].isdigit() else "li"
            text = m_li.group(3).strip()
            if text.startswith("#### ") or re.match(r"#{1,6} ", text):
                kind += "+h"
                text = re.sub(r"^#{1,6} ", "", text)
            out.append((f"{kind}{lvl}", text))
        elif indents and ln.startswith(" ") and ln.strip() and out and out[-1][0].startswith(("li", "oli")):
            k, t = out[-1]
            out[-1] = (k, t + " " + ln.strip())
        else:
            if ln.strip():
                indents = [] if not ln.startswith(" ") else indents
            m = re.match(r"(#{1,6}) (.+)", ln)
            if m:
                out.append(("h%d" % len(m.group(1)), m.group(2).strip()))
            elif re.match(r"!\[\[|!\[", ln.strip()):
                out.append(("img", ln.strip()))
            elif ln.strip().startswith("|"):
                if not re.match(r"\|\s*-", ln.strip()):
                    out.append(("table", ln.strip()))
            elif ln.strip().startswith(">"):
                out.append(("quote", ln.strip(" >")))
            elif ln.strip():
                out.append(("p", ln.strip()))
        i += 1
    return out


def ratio(a, b):
    return SequenceMatcher(None, a, b, autojunk=False).ratio()


def lev(a, b):
    if len(a) < len(b):
        a, b = b, a
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def cer_text(exp, got):
    we, wg = exp.split(" "), got.split(" ")
    sm = SequenceMatcher(None, we, wg, autojunk=False)
    errs = 0
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            continue
        a, b = " ".join(we[i1:i2]), " ".join(wg[j1:j2])
        if len(a) * len(b) > 4_000_000:
            errs += max(len(a), len(b))
        else:
            errs += lev(a, b)
    return errs / max(1, len(exp))


def match_code(ce, cg):
    pairs, used = [], set()
    for k, c in enumerate(ce):
        best, bi = 0.0, -1
        for j, d in enumerate(cg):
            if j in used:
                continue
            r = ratio(c, d)
            if r > best:
                best, bi = r, j
        if bi >= 0 and best >= 0.3:
            used.add(bi)
            pairs.append((k, bi, best))
        else:
            pairs.append((k, -1, 0.0))
    return pairs, [j for j in range(len(cg)) if j not in used]


def iou(a, b):
    ix = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    inter = ix * iy
    ua = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / ua if ua > 0 else 0.0


def match_figures(exp, got, thr=0.5):
    cand = sorted(((iou(e["rect"], g["rect"]), i, j) for i, e in enumerate(exp) for j, g in enumerate(got)), reverse=True)
    pe, pg, pairs = set(), set(), []
    for v, i, j in cand:
        if v < thr:
            break
        if i in pe or j in pg:
            continue
        pe.add(i)
        pg.add(j)
        pairs.append((i, j, v))
    missed = [i for i in range(len(exp)) if i not in pe]
    extra = [j for j in range(len(got)) if j not in pg]
    return pairs, missed, extra


def load_figures(path):
    data = json.load(open(path, encoding="utf-8"))
    if isinstance(data, dict):
        data = data["images"]
    return [d if isinstance(d, dict) else {"rect": d, "kind": "?"} for d in data]


def tables_of(md):
    out, cur = [], []
    for ln in strip_yaml(md).split("\n") + [""]:
        t = ln.strip()
        if t.startswith("|") and t.endswith("|") and len(t) > 1:
            cells = [c.strip() for c in re.split(r"(?<!\\)\|", t[1:-1])]
            if not all(re.fullmatch(r":?-{3,}:?", c) for c in cells if c):
                cur.append(cells)
        elif cur:
            out.append(cur)
            cur = []
    return out


def table_score(te, tg):
    norm_c = lambda c: " ".join(c.split())
    used, total, ok, details = set(), 0, 0, []
    for k, a in enumerate(te):
        n = sum(len(r) for r in a)
        total += n
        best, bj = -1, -1
        for j, b in enumerate(tg):
            if j in used:
                continue
            hit = sum(1 for ri, r in enumerate(a) for ci, c in enumerate(r)
                      if ri < len(b) and ci < len(b[ri]) and norm_c(b[ri][ci]) == norm_c(c))
            if hit > best:
                best, bj = hit, j
        if bj >= 0 and best > 0:
            used.add(bj)
            ok += best
            details.append((k + 1, best, n, (len(tg[bj]), max(len(r) for r in tg[bj]))))
        else:
            details.append((k + 1, 0, n, None))
    return (ok / total if total else 1.0), details, len(tg) - len(used)


def evaluate(exp, got, fig_got=None, fig_exp=None):
    r = {}
    we, wg = words(exp), words(got)
    sm = SequenceMatcher(None, we, wg, autojunk=False)
    matched = sum(b.size for b in sm.get_matching_blocks())
    r["recall"] = matched / max(1, len(we))
    r["precision"] = matched / max(1, len(wg))
    r["f1"] = 2 * r["recall"] * r["precision"] / max(1e-9, r["recall"] + r["precision"])
    r["n_words"] = (len(we), len(wg))
    r["cer_text"] = cer_text(prose(exp), prose(got))

    ce, cg = code_blocks(exp), code_blocks(got)
    pairs, extra_code = match_code(ce, cg)
    tot = sum(len(c) for c in ce)
    errs = 0
    r["code_pairs"] = []
    for k, j, sim in pairs:
        e = lev(ce[k], cg[j]) if j >= 0 else len(ce[k])
        errs += min(e, len(ce[k]))
        r["code_pairs"].append({"block": k + 1, "chars": len(ce[k]), "sim": sim, "cer": e / max(1, len(ce[k])),
                                "head": ce[k].split("\n")[0][:50]})
    r["cer_code"] = errs / max(1, tot) if ce else 0.0
    r["code"] = (len(ce), len(cg), len(extra_code))

    be, bg = blocks(exp), blocks(got)
    te, tg = [k for k, _ in be], [k for k, _ in bg]
    r["structure"] = ratio(te, tg)
    r["tables"] = table_score(tables_of(exp), tables_of(got))
    r["blocks"] = (be, bg)

    def count(bs, pred):
        return sum(1 for k, _ in bs if pred(k))

    r["counts"] = {}
    for lab, pred in [
        ("Headings", lambda k: re.fullmatch(r"h\d", k)),
        ("Images", lambda k: k == "img"),
        ("Bullets", lambda k: k.startswith("li")),
        ("  level 2", lambda k: k.startswith("li") and k.endswith("1")),
        ("  with ####", lambda k: k.startswith("li+h")),
        ("Num.", lambda k: k.startswith("oli")),
        ("Quotes", lambda k: k == "quote"),
        ("Tables", lambda k: k == "table"),
    ]:
        r["counts"][lab] = (count(be, pred), count(bg, pred))
    he = [(k, norm(t)) for k, t in be if re.fullmatch(r"h\d", k)]
    hg = [(k, norm(t)) for k, t in bg if re.fullmatch(r"h\d", k)]
    r["headings_exact"] = sum(1 for h in he if h in hg) / max(1, len(he))

    if fig_got is not None and fig_exp is not None:
        pairs, missed, extra = match_figures(fig_exp, fig_got)
        r["fig"] = {
            "expected": len(fig_exp),
            "got": len(fig_got),
            "matched": len(pairs),
            "missed": [fig_exp[i]["kind"] for i in missed],
            "extra": [list(map(round, fig_got[j]["rect"])) for j in extra],
            "iou_mean": sum(v for _, _, v in pairs) / max(1, len(fig_exp)),
            "pairs": [(fig_exp[i]["kind"], round(v, 3)) for i, _, v in pairs],
        }
        r["fig"]["score"] = sum(v for _, _, v in pairs) / max(1, len(fig_exp) + len(extra))
    parts = [(0.30, r["f1"] * (1 - min(1.0, r["cer_text"]))), (0.20, r["structure"])]
    if ce:
        parts.append((0.20, 1 - min(1.0, r["cer_code"])))
    if "fig" in r:
        parts.append((0.30, r["fig"]["score"]))
    r["score"] = sum(w * v for w, v in parts) / sum(w for w, _ in parts)
    return r


def report(r):
    print(f"TEXT   recall {r['recall']:.3f}  precision {r['precision']:.3f}  F1 {r['f1']:.3f}  "
          f"CER {r['cer_text']:.3f}  (target words {r['n_words'][0]}, got {r['n_words'][1]})")
    for lab, (ce, cg) in r["counts"].items():
        flag = "" if ce == cg else "  <-- mismatch"
        print(f"  {lab:11} target {ce:3}  got {cg:3}{flag}")
    print(f"  Exact headings (text + level) {r['headings_exact']:.2f}   structure {r['structure']:.3f}")
    sc, det, extra_t = r["tables"]
    if det or extra_t:
        print(f"\nTABLES  exact cells {sc:.3f}   extra tables {extra_t}")
        for k, hit, n, shape in det:
            print(f"  table {k}: {hit}/{n} cells" + (f" (got {shape[0]}x{shape[1]})" if shape else " (missing)"))
    n_e, n_g, n_x = r["code"]
    print(f"\nCODE   target blocks {n_e}  got {n_g}  extra {n_x}   CER {r['cer_code']:.3f}")
    for p in r["code_pairs"]:
        print(f"  block {p['block']} (~{p['chars']} chars) similarity {p['sim']:.2f}  CER {p['cer']:.3f}  \"{p['head']}\"")
    if "fig" in r:
        f = r["fig"]
        print(f"\nIMAGES expected {f['expected']}  got {f['got']}  matched (IoU>=0.5) {f['matched']}  "
              f"mean IoU {f['iou_mean']:.3f}  framing {f['score']:.3f}")
        for k, v in f["pairs"]:
            print(f"  {k:14} IoU {v:.3f}")
        for k in f["missed"]:
            print(f"  missed: {k}")
        for rect in f["extra"]:
            print(f"  extra: {rect}")
    print(f"\nSCORE GLOBAL {r['f1']:.3f}   SCORE COMPOSITE {r['score']:.3f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cible")
    ap.add_argument("got")
    ap.add_argument("--figures", help="rectangles found (markdownshot --figures)")
    ap.add_argument("--expected-figures", help="expected rectangles (figures.json from capture.py)")
    ap.add_argument("--json", action="store_true", help="JSON result on standard output")
    a = ap.parse_args()
    exp = open(a.cible, encoding="utf-8").read()
    got = open(a.got, encoding="utf-8").read()
    fg = load_figures(a.figures) if a.figures else None
    fe = load_figures(a.expected_figures) if a.expected_figures else None
    r = evaluate(exp, got, fg, fe)
    if a.json:
        r.pop("blocks")
        json.dump(r, sys.stdout, ensure_ascii=False)
        print()
    else:
        report(r)


if __name__ == "__main__":
    main()
