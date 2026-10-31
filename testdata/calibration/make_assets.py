from pathlib import Path
import random

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).parent
OUT = HERE / "assets"
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_B = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def diagram(dark: bool) -> str:
    ink = "#c9d1d9" if dark else "#3f3f46"
    soft = "#8b949e" if dark else "#71717a"
    accent = "#58a6ff" if dark else "#2563eb"
    w, h = 720, 380
    boxes = [
        (4, 40, 150, 56, "Ingestio"),
        (285, 40, 150, 56, "Nucleus"),
        (566, 40, 150, 56, "Archivum"),
        (285, 190, 150, 56, "Cache lorem"),
        (566, 190, 150, 56, "Index ipsum"),
    ]
    s = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
         f'font-family="DejaVu Sans, sans-serif">']
    s.append(f'<defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" '
             f'orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="{ink}"/></marker></defs>')
    s.append(f'<text x="4" y="16" font-size="14" fill="{soft}">Figura 1 — fluxus datorum</text>')
    for x, y, bw, bh, label in boxes:
        s.append(f'<rect x="{x}" y="{y}" width="{bw}" height="{bh}" rx="6" fill="none" stroke="{ink}" stroke-width="1.5"/>')
        s.append(f'<text x="{x + bw / 2}" y="{y + bh / 2 + 5}" font-size="15" text-anchor="middle" fill="{ink}">{label}</text>')
    arrows = [
        (154, 68, 281, 68, "dolor", False),
        (435, 68, 562, 68, "sit amet", False),
        (360, 96, 360, 186, "consectetur", True),
        (435, 218, 562, 218, "adipiscing", False),
        (641, 186, 641, 100, "", True),
    ]
    for x1, y1, x2, y2, label, dashed in arrows:
        dash = ' stroke-dasharray="6 5"' if dashed else ""
        s.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{ink}" stroke-width="1.5"{dash} marker-end="url(#a)"/>')
        if label:
            if y1 == y2:
                s.append(f'<text x="{(x1 + x2) / 2}" y="{y1 - 8}" font-size="13" text-anchor="middle" fill="{soft}">{label}</text>')
            else:
                s.append(f'<text x="{x1 + 10}" y="{(y1 + y2) / 2 + 4}" font-size="13" fill="{soft}">{label}</text>')
    s.append(f'<path d="M 79 96 C 79 160, 200 218, 281 218" fill="none" stroke="{accent}" stroke-width="1.5" marker-end="url(#a)"/>')
    s.append(f'<text x="96" y="176" font-size="13" fill="{accent}">reditus</text>')
    ly = h - 40
    s.append(f'<line x1="4" y1="{ly}" x2="44" y2="{ly}" stroke="{ink}" stroke-width="1.5"/>')
    s.append(f'<text x="52" y="{ly + 5}" font-size="13" fill="{soft}">fluxus principalis</text>')
    s.append(f'<line x1="220" y1="{ly}" x2="260" y2="{ly}" stroke="{ink}" stroke-width="1.5" stroke-dasharray="6 5"/>')
    s.append(f'<text x="268" y="{ly + 5}" font-size="13" fill="{soft}">imperium</text>')
    s.append(f'<line x1="400" y1="{ly}" x2="440" y2="{ly}" stroke="{accent}" stroke-width="1.5"/>')
    s.append(f'<text x="448" y="{ly + 5}" font-size="13" fill="{soft}">reditus erroris</text>')
    s.append(f'<text x="4" y="{h - 6}" font-size="12" fill="{soft}">Nota : magnitudines non ad scalam delineatae sunt.</text>')
    s.append("</svg>")
    return "\n".join(s)


def photo() -> None:
    rnd = random.Random(11)
    w, h = 1200, 700
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=(int(40 + 150 * t), int(90 + 90 * t), int(160 + 40 * t)))
    for _ in range(30):
        x, y, r = rnd.randint(0, w), rnd.randint(0, 260), rnd.randint(30, 110)
        d.ellipse([x - r * 2, y - r // 2, x + r * 2, y + r // 2], fill=(235, 238, 245))
    img = img.filter(ImageFilter.GaussianBlur(18))
    d = ImageDraw.Draw(img)
    for col, base, amp in [((60, 110, 70), 420, 60), ((40, 85, 50), 470, 45), ((30, 60, 40), 520, 30)]:
        pts = [(0, h)] + [(x, base - int(amp * (0.5 + 0.5 * __import__("math").sin(x / (90 + amp) + amp)))) for x in range(0, w + 1, 12)] + [(w, h)]
        d.polygon(pts, fill=col)
    d.rectangle([0, 560, w, h], fill=(70, 110, 150))
    img = img.filter(ImageFilter.GaussianBlur(1.5))
    px = img.load()
    for y in range(h):
        for x in range(w):
            n = rnd.randint(-9, 9)
            p = px[x, y]
            px[x, y] = (max(0, min(255, p[0] + n)), max(0, min(255, p[1] + n)), max(0, min(255, p[2] + n)))
    img.save(OUT / "photo.jpg", quality=88)


def chart(dark: bool) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fg = "#c9d1d9" if dark else "#334155"
    grid = "#30363d" if dark else "#e2e8f0"
    bg = "#161b22" if dark else "#ffffff"
    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 12})
    fig, ax = plt.subplots(figsize=(8.0, 3.8), dpi=100)
    fig.patch.set_facecolor(bg)
    ax.set_facecolor(bg)
    xs = list(range(12))
    a = [12, 15, 14, 19, 23, 22, 27, 31, 29, 35, 38, 41]
    b = [8, 9, 12, 11, 13, 17, 16, 18, 22, 21, 25, 24]
    ax.plot(xs, a, color="#3b82f6", lw=2.2, marker="o", ms=4, label="Lorem")
    ax.plot(xs, b, color="#f97316", lw=2.2, marker="s", ms=4, label="Ipsum")
    ax.fill_between(xs, b, color="#f97316", alpha=0.12)
    ax.set_xticks(xs, ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII"])
    ax.set_title("Mensura per mensem", color=fg, loc="left", fontsize=14, fontweight="bold")
    ax.set_ylabel("unitates", color=fg)
    ax.grid(color=grid, lw=1)
    for sp in ax.spines.values():
        sp.set_color(grid)
    ax.tick_params(colors=fg)
    leg = ax.legend(frameon=False, loc="upper left")
    for t in leg.get_texts():
        t.set_color(fg)
    fig.tight_layout(pad=0.6)
    fig.savefig(OUT / f"chart-{'dark' if dark else 'light'}.png", facecolor=bg)
    plt.close(fig)


CODE_SNIPPET = [
    ("kw", "fn "), ("fn", "lorem_ipsum"), ("p", "(dolor: &[u8], sit: usize) -> Option<Vec<u32>> {"),
    None,
    ("p", "    "), ("kw", "let "), ("p", "mut amet = Vec::with_capacity(sit);"),
    None,
    ("p", "    "), ("kw", "for "), ("p", "(i, b) "), ("kw", "in "), ("p", "dolor.iter().enumerate() {"),
    None,
    ("p", "        "), ("kw", "if "), ("p", "*b == "), ("num", "0x1F"), ("p", " { "), ("kw", "continue"), ("p", "; }"),
    None,
    ("p", "        amet.push((i as u32) << "), ("num", "4"), ("p", ");"),
    None,
    ("p", "    }"),
    None,
    ("p", "    "), ("cm", "// consectetur adipiscing elit"),
    None,
    ("p", "    Some(amet)"),
    None,
    ("p", "}"),
]

CODE_SNIPPET_2 = [
    ("kw", "def "), ("fn", "elit_sed"), ("p", "(tempor, incididunt=None):"), None,
    ("p", "    "), ("st", '"""Ut labore et dolore magna."""'), None,
    ("p", "    data = {k: v "), ("kw", "for "), ("p", "k, v "), ("kw", "in "), ("p", "tempor.items() "), ("kw", "if "), ("p", "v}"), None,
    ("p", "    "), ("kw", "return "), ("p", "[x["), ("num", "0"), ("p", "] "), ("kw", "for "), ("p", "x "), ("kw", "in "), ("p", "sorted(data.items())]"), None,
]


def editor_html(tokens, dark: bool) -> str:
    pal = (
        {"bg": "#1e1e1e", "p": "#d4d4d4", "kw": "#569cd6", "fn": "#dcdcaa", "num": "#b5cea8", "cm": "#6a9955",
         "st": "#ce9178", "gut": "#858585", "bar": "#2d2d2d", "title": "#cccccc"}
        if dark else
        {"bg": "#ffffff", "p": "#24292f", "kw": "#cf222e", "fn": "#8250df", "num": "#0550ae", "cm": "#6e7781",
         "st": "#0a3069", "gut": "#8c959f", "bar": "#f6f8fa", "title": "#57606a"}
    )
    lines, cur = [], []
    for t in tokens:
        if t is None:
            lines.append(cur)
            cur = []
        else:
            cur.append(t)
    if cur:
        lines.append(cur)
    rows = []
    for i, ln in enumerate(lines, 1):
        spans = "".join(
            f'<span style="color:{pal[k]}">{v.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")}</span>'
            for k, v in ln
        )
        rows.append(f'<div class="r"><span class="g">{i}</span>{spans}</div>')
    border = "#3c3c3c" if dark else "#d0d7de"
    return f"""<!doctype html><html><head><meta charset="utf-8"><style>
    body {{ margin:0; background:transparent; }}
    .ed {{ display:inline-block; background:{pal['bg']}; border:1px solid {border}; border-radius:8px; overflow:hidden;
           font-family:"DejaVu Sans Mono", monospace; font-size:13px; line-height:20px; }}
    .bar {{ background:{pal['bar']}; color:{pal['title']}; font-family:"DejaVu Sans", sans-serif; font-size:12px;
            padding:6px 12px; border-bottom:1px solid {border}; }}
    .bar i {{ display:inline-block; width:10px; height:10px; border-radius:50%; margin-right:6px; vertical-align:-1px; }}
    .code {{ padding:10px 18px 12px 0; white-space:pre; color:{pal['p']}; }}
    .g {{ display:inline-block; width:34px; text-align:right; padding-right:14px; color:{pal['gut']}; }}
    </style></head><body><div class="ed"><div class="bar"><i style="background:#ff5f57"></i><i style="background:#febc2e"></i><i style="background:#28c840"></i>&nbsp; lorem.rs</div>
    <div class="code">{''.join(rows)}</div></div></body></html>"""


DIALOG_HTML = """<!doctype html><html><head><meta charset="utf-8"><style>
body { margin:0; background:transparent; font-family:"DejaVu Sans", sans-serif; font-size:13px; color:#1f2328; }
.win { display:inline-block; width:430px; background:#f3f3f3; border:1px solid #9a9a9a; box-shadow:0 4px 14px rgba(0,0,0,.25); }
.title { background:#ffffff; padding:8px 12px; font-size:12px; border-bottom:1px solid #d9d9d9; display:flex; }
.title span:last-child { margin-left:auto; letter-spacing:12px; color:#555; }
.tabs { display:flex; gap:2px; padding:8px 10px 0; }
.tabs div { padding:5px 12px; background:#e5e5e5; border:1px solid #c8c8c8; border-bottom:none; }
.tabs div.on { background:#fff; }
.body { background:#fff; margin:0 10px; border:1px solid #c8c8c8; padding:14px; }
.row { display:flex; align-items:center; margin:8px 0; }
.row label { width:110px; }
.inp { flex:1; border:1px solid #8a8a8a; background:#fff; padding:4px 6px; }
.chk { margin:6px 0; } .chk b { display:inline-block; width:12px; height:12px; border:1px solid #555; margin-right:8px; vertical-align:-2px; }
.chk b.on { background:#0067c0; border-color:#0067c0; }
.btns { display:flex; justify-content:flex-end; gap:8px; padding:12px 10px; }
.btns div { min-width:72px; text-align:center; padding:5px 10px; border:1px solid #8a8a8a; background:#fdfdfd; }
.btns div.def { border:2px solid #0067c0; }
</style></head><body><div class="win">
<div class="title"><span>Proprietates — Lorem ipsum</span><span>–□×</span></div>
<div class="tabs"><div class="on">Generalia</div><div>Securitas</div><div>Detalia</div></div>
<div class="body">
 <div class="row"><label>Nomen :</label><div class="inp">dolor_sit_amet.cfg</div></div>
 <div class="row"><label>Locus :</label><div class="inp">C:\\Lorem\\Ipsum\\Dolor</div></div>
 <div class="row"><label>Magnitudo :</label><div>12,4 Mo (13 002 880 octeti)</div></div>
 <div class="chk"><b class="on"></b>Tantum legere</div>
 <div class="chk"><b></b>Occultum</div>
</div>
<div class="btns"><div class="def">OK</div><div>Annuler</div><div>Appliquer</div></div>
</div></body></html>"""


def render_html(page, html: str, path: Path, selector: str) -> None:
    page.set_content(html)
    page.wait_for_timeout(50)
    page.locator(selector).screenshot(path=str(path), omit_background=True)


TABLE_HTML = """<!doctype html><html><head><meta charset="utf-8"><style>
body { margin:0; background:#ececec; color:#1f1f1f; font-family:"DejaVu Sans", sans-serif; }
main { width:900px; padding:16px 10px 10px; }
p { font-size:16px; line-height:1.6; margin:0 0 14px; }
table { border-collapse:collapse; width:100%; font-size:15px; line-height:1.55; }
th, td { border:1px solid #3a3a3a; padding:9px 12px; text-align:left; vertical-align:top; }
th { background:#d6d6d6; }
code { font-family:"DejaVu Sans Mono", monospace; font-size:14px; background:#e0e0e0; padding:1px 5px; border-radius:3px; }
</style></head><body><main>
<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.</p>
<table id="t">
<tr><th>Exemplum</th><th>Associatio inventa</th><th>Punctum ingressus</th><th>Observatio</th></tr>
<tr><td><code>lorem-ipsum</code></td><td><code>DelegataExecutio</code> sub schemate</td><td><code>Obiectum.Ingressus</code></td><td>Linea iussi servitoris, nulla URI</td></tr>
<tr><td><code>dolor-sit</code></td><td>Registratio fasciculata, <code>DelegataExecutio</code> sub ProgID</td><td><code>Amet.ApplicatioPlena</code></td><td>URI in linea iussi</td></tr>
</table></main></body></html>"""


CHECK_SVG = (
    '<svg width="22" height="22" viewBox="0 0 22 22"><circle cx="11" cy="11" r="9.5" fill="none" stroke="#5a6b4f" '
    'stroke-width="1.6"/><path d="M6.5 11.5 L9.8 14.6 L15.6 7.8" fill="none" stroke="#5a6b4f" stroke-width="1.8"/></svg>'
)

TABLE_VARIANTS = {
    "table": ("", "<code>dolor-sit</code>", "URI in linea iussi"),
    "table-checks": ("", "<code>dolor-sit</code>", CHECK_SVG),
    "table-frame": ("frame", "<code>dolor-sit</code>", "URI in linea iussi"),
}


def table_shot(page) -> None:
    for name, (frame, code, last) in TABLE_VARIANTS.items():
        html = TABLE_HTML.replace("<code>dolor-sit</code>", code).replace("<td>URI in linea iussi</td>", f"<td>{last}</td>")
        if frame:
            para = "<p>" + " ".join(["Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium."] * 3) + "</p>"
            html = html.replace('<table id="t">', '<div style="background:#e2dccb;padding:18px;border-radius:8px"><table id="t">').replace(
                "</table>", "</table></div>" + para * 4
            ).replace("<main>", "<main>" + para * 3)
        page.set_viewport_size({"width": 920, "height": 400})
        page.set_content(html)
        page.wait_for_timeout(50)
        r = page.evaluate("(() => { const b = document.getElementById('t').getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; })()")
        h = int(page.evaluate("document.body.scrollHeight")) if frame else int(r[3]) + 8
        page.set_viewport_size({"width": 920, "height": max(400, h)})
        page.screenshot(path=str(HERE / f"{name}.png"), clip={"x": 0, "y": 0, "width": 920, "height": h})
        if name == "table":
            (HERE / "table.json").write_text("[" + ", ".join(str(round(v)) for v in r) + "]\n")
            md = page.evaluate("""(() => {
              const cell = c => [...c.childNodes].map(n => n.nodeType === 3 ? n.textContent
                : (n.tagName === 'CODE' ? '`' + n.textContent + '`' : n.textContent)).join('').replace(/\\s+/g, ' ').trim();
              const rows = [...document.querySelectorAll('#t tr')].map(tr => [...tr.children].map(cell));
              const line = r => '| ' + r.join(' | ') + ' |';
              return [document.querySelector('p').textContent.trim(), '',
                      line(rows[0]), '|' + ' --- |'.repeat(rows[0].length), ...rows.slice(1).map(line)].join('\\n') + '\\n';
            })()""")
            (HERE / "table.md").write_text(md)


def annotate_dialog() -> None:
    src = Image.open(OUT / "dialog_raw.png").convert("RGBA")
    pad_r, pad = 230, 6
    W, H = src.width + pad_r, src.height + 2 * pad
    img = Image.new("RGBA", (W, H), (255, 255, 255, 255))
    img.alpha_composite(src, (pad, pad))
    d = ImageDraw.Draw(img)
    red = (220, 38, 38, 255)
    font = ImageFont.truetype(FONT_B, 14)
    small = ImageFont.truetype(FONT, 13)
    targets = [(pad + 130, pad + 86, pad + 410, pad + 112), (pad + 22, pad + 178, pad + 142, pad + 200), (pad + 118, pad + 245, pad + 194, pad + 278)]
    notes = ["Nomen tabulae", "Tantum legere", "Confirmare"]
    for k, ((x0, y0, x1, y1), note) in enumerate(zip(targets, notes), 1):
        d.rounded_rectangle([x0, y0, x1, y1], radius=4, outline=red, width=3)
        ty = (y0 + y1) // 2
        cx = src.width + pad + 40
        d.line([(x1 + 2, ty), (cx - 14, ty)], fill=red, width=3)
        d.polygon([(cx - 14, ty - 6), (cx - 4, ty), (cx - 14, ty + 6)], fill=red)
        d.ellipse([cx, ty - 12, cx + 24, ty + 12], fill=red)
        d.text((cx + 12, ty), str(k), font=font, fill="white", anchor="mm")
        d.text((cx + 32, ty), note, font=small, fill=red, anchor="lm")
    bbox = Image.eval(img.convert("L"), lambda v: 255 if v < 250 else 0).getbbox()
    img = img.crop((max(0, bbox[0] - 3), max(0, bbox[1] - 3), min(W, bbox[2] + 3), min(H, bbox[3] + 3)))
    img.convert("RGB").save(OUT / "dialog.png")
    (OUT / "dialog_raw.png").unlink()


def levels() -> None:
    w, h = 1200, 640
    bgc = (240, 238, 230)
    img = Image.new("RGB", (w, h), bgc)
    d = ImageDraw.Draw(img)
    f = ImageFont.truetype(FONT, 22)
    fb = ImageFont.truetype(FONT_B, 26)
    fs = ImageFont.truetype(FONT, 18)
    rows = ["Usus defensivus", "Probatio penetrationis", "Inquisitio vulnerabilitatum", "Retentio datorum", "Recensio manualis"]
    tiers = [("Defensio", [1, 0, 1, 1, 0]), ("Probatio", [1, 1, 1, 1, 0]), ("Specialis", [1, 1, 1, 1, 1])]
    x0, colw = 360, 270
    d.text((40, 40), "Gradus accessus", font=fb, fill=(20, 20, 19))
    for k, (name, marks) in enumerate(tiers):
        cx = x0 + k * (colw + 10)
        d.rounded_rectangle([cx, 30, cx + colw, h - 30], radius=14, fill=(250, 249, 245), outline=(214, 211, 199), width=2)
        d.text((cx + colw // 2, 70), name, font=fb, fill=(20, 20, 19), anchor="mm")
        for i, m in enumerate(marks):
            y = 150 + i * 92
            if m:
                d.line([(cx + colw // 2 - 14, y), (cx + colw // 2 - 4, y + 12), (cx + colw // 2 + 16, y - 12)], fill=(204, 120, 92), width=5)
            else:
                d.line([(cx + colw // 2 - 12, y), (cx + colw // 2 + 12, y)], fill=(160, 157, 148), width=4)
    for i, r in enumerate(rows):
        y = 150 + i * 92
        d.text((40, y), r, font=f, fill=(40, 40, 38), anchor="lm")
        if i < len(rows) - 1:
            d.line([(40, y + 46), (w - 40, y + 46)], fill=(214, 211, 199), width=1)
    d.text((40, h - 46), "Nota : gradus superior inferiores continet.", font=fs, fill=(110, 108, 100))
    img.save(OUT / "levels.png")


def icons() -> None:
    icons = {
        "info": '<circle cx="10" cy="10" r="9" fill="#2563eb"/><rect x="9" y="8.5" width="2" height="6" fill="#fff"/><circle cx="10" cy="5.6" r="1.3" fill="#fff"/>',
        "alert": '<path d="M10 1.5 L19 18 H1 Z" fill="#f59e0b"/><rect x="9" y="7" width="2" height="6" fill="#fff"/><circle cx="10" cy="15.2" r="1.2" fill="#fff"/>',
        "ok": '<circle cx="10" cy="10" r="9" fill="#16a34a"/><path d="M5.5 10.5 L8.6 13.4 L14.5 7" stroke="#fff" stroke-width="2.2" fill="none"/>',
        "gear": '<circle cx="10" cy="10" r="6" fill="none" stroke="#6b7280" stroke-width="3" stroke-dasharray="3 1.7"/><circle cx="10" cy="10" r="2.4" fill="#6b7280"/>',
    }
    for name, body in icons.items():
        (OUT / f"icon-{name}.svg").write_text(
            f'<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">{body}</svg>\n'
        )


def main() -> None:
    OUT.mkdir(exist_ok=True)
    (OUT / "diagram-light.svg").write_text(diagram(False))
    (OUT / "diagram-dark.svg").write_text(diagram(True))
    photo()
    chart(False)
    chart(True)
    levels()
    icons()
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        b = p.chromium.launch()
        page = b.new_page(device_scale_factor=1)
        render_html(page, editor_html(CODE_SNIPPET, True), OUT / "code-shot-dark.png", ".ed")
        render_html(page, editor_html(CODE_SNIPPET_2, False), OUT / "code-shot-light.png", ".ed")
        render_html(page, DIALOG_HTML, OUT / "dialog_raw.png", ".win")
        table_shot(page)
        b.close()
    annotate_dialog()
    print("assets:", ", ".join(sorted(f.name for f in OUT.iterdir())))


if __name__ == "__main__":
    main()
