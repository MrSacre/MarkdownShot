#!/usr/bin/env python3
import shutil
import argparse
import re
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = [
    "det.onnx", "det_tiny.onnx", "rec.onnx", "rec_tiny.onnx", "rec_tiny_dict.txt", "layout.onnx",
    "onnxruntime.dll", "onnxruntime_providers_shared.dll", "DirectML.dll", "msvcp140.dll", "msvcp140_1.dll", "vcruntime140.dll", "vcruntime140_1.dll",
]

README = """MarkdownShot: screenshot to Markdown, 100% local
==============================================

Run markdownshot.exe: an icon appears next to the clock (notification area).
On first launch, pick the folder where your notes are saved (for example a
folder in your Obsidian vault).

  Ctrl+Alt+S   Capture: choose Zone, Window or Full screen in the toolbar,
               draw the area, then mark the parts to keep as images.
  Ctrl+Alt+R   Record a scrolling page: draw the area, MarkdownShot scrolls it
               (or scroll it yourself), Esc to finish.

The .md note is written to the notes folder, its images to the "images"
subfolder, and the Markdown is also copied to the clipboard.

Settings: right-click the icon, then Settings.
Configuration file: %APPDATA%\\MarkdownShot\\config.toml
Portable use: create an empty markdownshot.toml next to the executable and the
configuration is kept there.
Log: %APPDATA%\\MarkdownShot\\markdownshot.log
Command line: markdownshot.exe --help

Keep all the files in this folder together (models and libraries).
Source code and documentation: https://github.com/MrSacre/MarkdownShot
"""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=str(ROOT / "target/release/markdownshot.exe"))
    ap.add_argument("--out", default=str(ROOT / "dist"))
    ap.add_argument("--stage", default="", help="folder to also copy the files to (installer)")
    args = ap.parse_args()

    cargo = (ROOT / "crates/app/Cargo.toml").read_text(encoding="utf-8")
    ws = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.M) or re.search(r'^version\s*=\s*"([^"]+)"', ws, re.M)
    version = m.group(1) if m else "0.0.0"

    exe = Path(args.exe)
    if not exe.is_file():
        raise SystemExit(f"executable not found: {exe}")
    models = ROOT / "models"
    missing = [f for f in FILES if not (models / f).is_file()]
    if missing:
        raise SystemExit(f"missing files in models/: {missing}\nrun: python scripts/fetch_models.py --runtime windows")

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    zpath = out / f"MarkdownShot-{version}-windows-x64-portable.zip"
    base = "MarkdownShot"
    entries: list[tuple[str, Path | str]] = [("markdownshot.exe", exe)]
    entries += [(f, models / f) for f in FILES]
    entries += [
        ("README.txt", README.replace("\n", "\r\n")),
        ("licenses/LICENSE-MarkdownShot.txt", ROOT / "LICENSE"),
        ("licenses/THIRD_PARTY.md", ROOT / "THIRD_PARTY.md"),
        ("licenses/LICENSE-LiberationSans.txt", ROOT / "assets/fonts/LICENSE-LiberationSans.txt"),
    ]
    with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for rel, src in entries:
            if isinstance(src, Path):
                z.write(src, f"{base}/{rel}")
            else:
                z.writestr(f"{base}/{rel}", src)
    print(f"{zpath}  ({zpath.stat().st_size / 1e6:.1f} MB)")
    if args.stage:
        stage = Path(args.stage)
        shutil.rmtree(stage, ignore_errors=True)
        for rel, src in entries:
            dst = stage / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(src, Path):
                shutil.copy2(src, dst)
            else:
                dst.write_text(src, encoding="utf-8")
        print(f"{stage}  ({len(entries)} files)")


if __name__ == "__main__":
    main()
