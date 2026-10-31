from pathlib import Path
import random

HERE = Path(__file__).parent

WORDS = (
    "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et "
    "dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco laboris nisi aliquip ex ea "
    "commodo consequat duis aute irure in reprehenderit voluptate velit esse cillum fugiat nulla pariatur "
    "excepteur sint occaecat cupidatat non proident sunt culpa qui officia deserunt mollit anim id est "
    "laborum curabitur pretium tincidunt lacus nunc mauris tellus sagittis porttitor vivamus vestibulum "
    "integer tristique morbi fermentum pellentesque habitant senectus netus fames turpis egestas"
).split()


class Lorem:
    def __init__(self, seed: int):
        self.r = random.Random(seed)

    def words(self, n: int) -> str:
        return " ".join(self.r.choice(WORDS) for _ in range(n))

    def sentence(self, lo=8, hi=16) -> str:
        s = self.words(self.r.randint(lo, hi))
        return s[0].upper() + s[1:] + "."

    def para(self, n=3, lo=8, hi=16) -> str:
        return " ".join(self.sentence(lo, hi) for _ in range(n))

    def tail(self, lo=6, hi=12) -> str:
        return self.words(self.r.randint(lo, hi)) + "."

    def title(self, n=3) -> str:
        s = self.words(n)
        return s[0].upper() + s[1:]


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def page(title: str, body: str) -> str:
    return f"""<!doctype html>
<html lang="la">
<head>
<meta charset="utf-8">
<meta name="color-scheme" content="light dark">
<title>{esc(title)}</title>
<link rel="stylesheet" href="style.css">
</head>
<body>
<main>
{body}
</main>
</body>
</html>
"""


def fig(src: str, kind: str, caption: str = "", cls: str = "", alt: str = "") -> str:
    cap = f"\n  <figcaption>{esc(caption)}</figcaption>" if caption else ""
    c = f' class="{cls}"' if cls else ""
    return f'<figure{c}>\n  <img data-figure="{kind}" src="{src}" alt="{esc(alt or kind)}">{cap}\n</figure>'


def fig_theme(stem: str, ext: str, kind: str, caption: str = "", cls: str = "") -> str:
    cap = f"\n  <figcaption>{esc(caption)}</figcaption>" if caption else ""
    c = f' class="{cls}"' if cls else ""
    return (
        f"<figure{c}>\n  <picture>\n"
        f'    <source media="(prefers-color-scheme: dark)" srcset="assets/{stem}-dark.{ext}">\n'
        f'    <img data-figure="{kind}" src="assets/{stem}-light.{ext}" alt="{kind}">\n'
        f"  </picture>{cap}\n</figure>"
    )


def shot(src: str, kind: str, caption: str = "") -> str:
    cap = f"\n  <figcaption>{esc(caption)}</figcaption>" if caption else ""
    return (
        f'<figure class="narrow">\n  <div class="shot"><img data-figure="{kind}" src="{src}" alt="{kind}">'
        f'<button class="fs" aria-label="full screen">⛶</button></div>{cap}\n</figure>'
    )


def pre(cls: str, code: str) -> str:
    return f'<pre class="{cls}"><code>{esc(code.strip(chr(10)))}</code></pre>'


def ul(items, cls: str = "") -> str:
    c = f' class="{cls}"' if cls else ""
    return f"<ul{c}>\n" + "\n".join(f"  <li>{i}</li>" for i in items) + "\n</ul>"


def ol(items) -> str:
    return "<ol>\n" + "\n".join(f"  <li>{i}</li>" for i in items) + "\n</ol>"


def table(head, rows) -> str:
    th = "".join(f"<th>{h}</th>" for h in head)
    trs = "\n".join("  <tr>" + "".join(f"<td>{c}</td>" for c in r) + "</tr>" for r in rows)
    return f'<table class="bordered">\n  <tr>{th}</tr>\n{trs}\n</table>'


def callout(html: str) -> str:
    return f'<div class="callout"><span class="ico">i</span><p>{html}</p></div>'


def glyph(name: str) -> str:
    return f'<img class="glyph" src="assets/icon-{name}.svg" alt="">'


CODE_RUST = """
pub struct Lorem<'a> {
    ipsum: &'a [u8],
    dolor: Vec<Option<u32>>,
}

impl<'a> Lorem<'a> {
    pub fn sit(&mut self, amet: usize) -> Result<(), Il1Error> {
        let l1 = self.ipsum.get(amet..).ok_or(Il1Error::O0)?;
        for (i, b) in l1.iter().enumerate() {
            if *b > 0x7F && i % 2 == 0 {
                self.dolor.push(Some(*b as u32));
            } else {
                self.dolor.push(None);
            }
        }
        Ok(())
    }
}
"""

CODE_JSON = """
{
  "lorem": "ipsum",
  "dolor": [10, 20, 0x0F],
  "sit": {
    "amet": true,
    "elit": ["sed", "do"],
    "tempor": null
  },
  "Il1": "O0o"
}
"""

CODE_SHELL = """
$ lorem --ipsum ./dolor/sit.toml --amet=0
[ok] consectetur: 12 elementa lecta
[ok] adipiscing: <none>
$ elit -v | tail -n 1
sed_do_eiusmod = { tempor: 1, incididunt: [0, 1] }
"""

CODE_CONSOLE = """
0:000> u lorem!ipsum+0x1a L6
lorem!ipsum+0x1a:
00007ff6`1a2b3c4a 488b4c2408      mov     rcx,qword ptr [rsp+8]
00007ff6`1a2b3c4f 4c8bc1          mov     r8,rcx
00007ff6`1a2b3c52 33d2            xor     edx,edx
00007ff6`1a2b3c54 e8a7f1ffff      call    lorem!dolor (00007ff6`1a2b2e00)
00007ff6`1a2b3c59 85c0            test    eax,eax
00007ff6`1a2b3c5b 7410            je      lorem!ipsum+0x3d (00007ff6`1a2b3c6d)
"""

CODE_CONSOLE_2 = """
Child-SP          RetAddr               Call Site
000000d1`4f5ff6a8 00007ff8`c3a21b2e     ntdll!NtWaitForSingleObject+0x14
000000d1`4f5ff6b0 00007ff6`1a2b3c59     KERNELBASE!WaitForSingleObjectEx+0x8e
000000d1`4f5ff750 00007ff6`1a2b4010     lorem!ipsum+0x39
000000d1`4f5ff790 00007ff8`c5c17344     lorem!main+0x120
"""

CODE_PY = """
class Dolor:
    def __init__(self, sit, amet=None):
        self.sit = sit
        self.amet = amet or {}

    def elit(self, *args, **kwargs):
        if not args:
            return [k for k, v in self.amet.items() if v]
        return {"sed": args[0], "do": len(args)}
"""

CODE_C = """
static int lorem_ipsum(const char *dolor, size_t sit)
{
    int amet[4] = { 0, 1, 1, 0 };
    for (size_t i = 0; i < sit; ++i) {
        if (dolor[i] == '\\0')
            break;
        amet[i & 3] ^= (int)dolor[i] << 1;
    }
    return amet[0] | amet[1] | amet[2];
}
"""


def page_doc() -> str:
    L = Lorem(101)
    b = []
    b.append(f"<h1>Lorem API : documentatio technica</h1>")
    b.append('<div class="meta">Versio 2.4 · ultima mutatio XII kal. nov.</div>')
    b.append(f"<p>{L.para(2)} Vide <code>lorem::init()</code> et {L.tail()}</p>")
    b.append("<h2>Installatio</h2>")
    b.append(f"<p>{L.para(2)}</p>")
    b.append(pre("dark", CODE_SHELL))
    b.append(ul([
        f"{L.sentence(6, 10)}",
        f"Optio <code>--amet</code> {L.words(8)}.",
        f"{L.sentence(10, 14)}",
    ]))
    b.append(fig_theme("diagram", "svg", "diagram", "Figura prima : fluxus datorum inter partes systematis."))
    b.append(f"<h3>{glyph('gear')}Configuratio</h3>")
    b.append(ol([
        f"{L.sentence(6, 9)}" + ul([L.sentence(5, 8), L.sentence(5, 8)]),
        f"{L.sentence(6, 9)}",
        f"{L.sentence(6, 9)}" + ul([f"Clavis <code>dolor</code> {L.words(5)}.", L.sentence(5, 8)]),
    ]))
    b.append(pre("light", CODE_JSON))
    b.append(callout(f"<strong>Nota bene :</strong> {L.sentence(12, 18)} {L.sentence(8, 12)}"))
    b.append("<h2>Diagnostica</h2>")
    b.append(f"<p>{L.sentence()} Iussum <code class=\"b\">u lorem!ipsum</code> {L.tail(8, 14)}</p>")
    b.append(pre("console", CODE_CONSOLE))
    b.append(ul([L.sentence(5, 9), L.sentence(5, 9), L.sentence(5, 9), L.sentence(5, 9)], "arrows"))
    b.append(shot("assets/code-shot-dark.png", "code-shot", "Exemplum in editore (imago)."))
    b.append(f"<p>{L.para(2)}</p>")
    b.append(f"<h4>Errores frequentes</h4>")
    b.append(f"<blockquote><p>{L.sentence(10, 16)}</p></blockquote>")
    b.append(fig_theme("chart", "png", "chart", "Figura secunda : mensura per mensem."))
    b.append(f"<p>{L.para(2)} Functio <code>sit()</code> {L.tail(6, 10)}</p>")
    b.append(pre("dark", CODE_RUST))
    b.append(f"<p>{L.para(1)}</p>")
    b.append("<h3>Tabula associationum</h3>")
    b.append(f"<p>{L.sentence(10, 14)}</p>")
    b.append(table(
        ["Exemplum", "Associatio", "Punctum ingressus", "Observatio"],
        [
            ["<code>lorem-ipsum</code>", "<code>DelegataExecutio</code> sub schemate", "<code>Obiectum.Ingressus</code>", "Linea iussi, nulla URI"],
            ["<code>dolor-sit</code>", f"Registratio {L.words(2)}, <code>DelegataExecutio</code> sub ProgID", "<code>Amet.Applicatio</code>", "URI in linea iussi"],
            ["<code>amet-elit</code>", f"{L.words(3)} <code>Fasciculus</code>", "<code>Elit.Sed</code>", f"{L.words(4)} tempor"],
        ],
    ))
    b.append(fig("assets/levels.png", "levels", "Conspectus graduum accessus."))
    b.append(f"<p>{L.para(1)}</p>")
    b.append(table(
        ["Gradus", "Usus", "Retentio"],
        [
            ["Defensio", f"{L.words(5)}", "<code>30 dies</code>"],
            ["Probatio", f"{L.words(6)}", "<code>30 dies</code>"],
            ["Specialis", f"{L.words(4)}", "<code>nulla</code>"],
        ],
    ))
    return page("Lorem API", "\n\n".join(b))


def page_tuto() -> str:
    L = Lorem(202)
    b = []
    b.append("<h1>Ipsum : ductus practicus</h1>")
    b.append(f"<p>{L.para(2)}</p>")
    b.append(fig("assets/photo.jpg", "photo", "Imago prima : vallis et lacus."))
    b.append("<h2>Primi gradus</h2>")
    b.append(f"<p>{L.para(2)}</p>")
    b.append(fig_theme("chart", "png", "chart", cls="tight"))
    b.append(f'<p class="compact">{L.sentence(8, 12)}</p>')
    b.append(fig("assets/dialog.png", "dialog", cls="tight"))
    b.append(f"<h3>{glyph('ok')}Optiones fenestrae</h3>")
    b.append(f"<p>{L.para(1)} Preme <code>OK</code> {L.tail(5, 8)}</p>")
    b.append(fig_theme("diagram", "svg", "diagram", cls="tight"))
    b.append(ul([L.sentence(5, 9), f"{L.words(4)} <code>reditus</code> {L.words(4)}.", L.sentence(5, 9)]))
    b.append(ol([L.sentence(5, 8), L.sentence(5, 8)]))
    b.append(pre("dark", CODE_PY))
    b.append("<h2>Comparatio</h2>")
    b.append(f"<p>{L.para(1)}</p>")
    b.append('<div class="duo">\n'
             '<figure><img data-figure="code-shot" src="assets/code-shot-light.png" alt="code"></figure>\n'
             '<figure><img data-figure="photo" src="assets/photo.jpg" alt="photo"></figure>\n</div>')
    b.append(table(
        ["Modus", "Iussum", "Effectus"],
        [
            ["<code>--lorem</code>", "<code>ipsum -d 0</code>", L.words(5)],
            ["<code>--sit</code>", f"{L.words(2)} <code>amet()</code>", L.words(4)],
        ],
    ))
    b.append(f"<p>{glyph('alert')}{L.para(2)}</p>")
    b.append(callout(f"{L.sentence(12, 16)} Vide <code>lorem.toml</code>."))
    b.append(shot("assets/code-shot-light.png", "code-shot", "Exemplum brevis (imago)."))
    b.append(ul([L.sentence(4, 8), L.sentence(4, 8), L.sentence(4, 8)], "arrows"))
    b.append(pre("light", CODE_C))
    b.append(f"<blockquote><p>{L.sentence(10, 14)}</p></blockquote>")
    b.append(fig("assets/dialog.png", "dialog", "Imago ultima : fenestra annotata."))
    b.append(f"<p>{L.para(2)}</p>")
    return page("Ipsum ductus", "\n\n".join(b))


def page_notes() -> str:
    L = Lorem(303)
    b = []
    b.append("<h1>Dolor : notae et indices</h1>")
    b.append(f"<p>{L.para(2)} Clavis <code>sit_amet</code> {L.tail(5, 8)}</p>")
    b.append("<h2>Structura</h2>")
    items = []
    for k in range(3):
        sub = ul([L.sentence(4, 7), f"{L.words(3)} <code class=\"b\">0x{k}F</code> {L.words(3)}.", L.sentence(4, 7)])
        items.append(f"<h4>{L.title(3)}</h4><p>{L.sentence(8, 12)}</p>{sub}")
    b.append(ul(items))
    b.append(pre("console", CODE_CONSOLE_2))
    b.append(fig("assets/dialog.png", "dialog", "Figura : optiones tabulae."))
    b.append(ol([
        L.sentence(5, 8) + ul([L.sentence(4, 7), L.sentence(4, 7)]),
        L.sentence(5, 8) + ul([L.sentence(4, 7)], "arrows"),
        L.sentence(5, 8),
    ]))
    b.append(f"<h3>{glyph('info')}Exempla codicis</h3>")
    b.append(pre("dark", CODE_C))
    b.append(fig_theme("diagram", "svg", "diagram", "Figura : partes et nexus."))
    b.append(f"<p>{L.para(2)} Vide <code>elit()</code>.</p>")
    b.append("<h2>Indices</h2>")
    items = []
    for k in range(2):
        sub = ul([L.sentence(4, 7), L.sentence(4, 7)])
        items.append(f"<h4>{L.title(2)}</h4><p>{L.sentence(8, 12)}</p>{sub}")
    b.append(ul(items))
    b.append(fig("assets/photo.jpg", "photo"))
    b.append(f"<h4>{L.title(3)}</h4>")
    b.append(f"<p>{L.para(2)}</p>")
    b.append(callout(f"<strong>Monitum :</strong> {L.sentence(10, 14)}"))
    b.append(pre("light", CODE_PY))
    b.append(f"<p>{L.para(1)}</p>")
    b.append(ul([L.sentence(4, 7), L.sentence(4, 7)]))
    b.append(table(
        ["Clavis", "Valor", "Nota"],
        [
            ["<code>sit_amet</code>", "<code>0x1F</code>", L.words(5)],
            ["<code>elit_sed</code>", "<code>0x2F</code>", L.words(4)],
            ["<code>tempor</code>", "<code>null</code>", f"{L.words(6)} {L.words(4)}"],
            ["<code>incididunt</code>", "<code>[0, 1]</code>", L.words(3)],
        ],
    ))
    return page("Dolor notae", "\n\n".join(b))


PAGES = {"doc": page_doc, "tuto": page_tuto, "notes": page_notes}


def main() -> None:
    for name, fn in PAGES.items():
        (HERE / f"{name}.html").write_text(fn(), encoding="utf-8")
    print("pages:", ", ".join(f"{n}.html" for n in PAGES))


if __name__ == "__main__":
    main()
