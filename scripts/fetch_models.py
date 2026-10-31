#!/usr/bin/env python3

import argparse
import hashlib
import io
import platform
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MODELS = ROOT / "models"

MODEL_WHEELS = [
    (
        "https://files.pythonhosted.org/packages/59/07/203065283bba0c5e9c10af3602dc25ab3e59ee54dc2d1d22c8faeabf5ed8/rapidocr-3.10.0-py3-none-any.whl",
        "2fc34e26cd0f48514804a94f8832283039c7334076bff60768b1fed9ad068050",
        {
            "rapidocr/models/PP-OCRv6_det_small.onnx": "det.onnx",
            "rapidocr/models/PP-OCRv6_rec_small.onnx": "rec.onnx",
        },
    ),
    (
        "https://files.pythonhosted.org/packages/92/fd/be4ce649c5fb4a59c028e4f5365bcb679e16abbc4da41baa4588e70a0ccd/onnxocr-4.0.0-py3-none-any.whl",
        "71c570a347ec688cd47dbc400c77e9abcbd3f48e497519234e0f5ce575c5845b",
        {
            "onnxocr/models/ppocrv6/tiny/det/det.onnx": "det_tiny.onnx",
            "onnxocr/models/ppocrv6/tiny/rec/rec.onnx": "rec_tiny.onnx",
            "onnxocr/models/ppocrv6/ppocrv6_tiny_dict.txt": "rec_tiny_dict.txt",
        },
    ),
    (
        "https://files.pythonhosted.org/packages/82/0f/81d8f160f2b44b3fe42edfd4565d330e1119d4f6f8481435be504999c447/rapid_layout-1.2.1-py3-none-any.whl",
        "7d424dcaa92c7cfd9c1c45bc6b323293a6461cf95cad605958fd0961f2397acf",
        {"rapid_layout/models/layout_cdla.onnx": "layout.onnx"},
    ),
]

RUNTIME_WHEELS = {
    "windows": [
        (
            "https://files.pythonhosted.org/packages/60/53/2bd2696fac19cf8ca55496a0bcfe431f3aff9579eabbb0e231dc238acf6f/onnxruntime_directml-1.24.4-cp313-cp313-win_amd64.whl",
            "2f1031cb2281e5b27cca9efe0b9399317c7286e4d226f7a79d4ab79bbd94d19e",
            {
                "onnxruntime/capi/onnxruntime.dll": "onnxruntime.dll",
                "onnxruntime/capi/onnxruntime_providers_shared.dll": "onnxruntime_providers_shared.dll",
                "onnxruntime/capi/DirectML.dll": "DirectML.dll",
            },
        ),
        (
            "https://files.pythonhosted.org/packages/fe/17/7a1eace5a7c6083bd99db861625c08bb37010dfbe90ed7a0194ae0281689/msvc_runtime-14.44.35112-cp313-cp313-win_amd64.whl",
            "d4f6cf106aaf235f2a90952f9ec7de49e9946323880d051abf9745a6d4bf60bf",
            {
                "msvc_runtime-14.44.35112.data/data/Scripts/msvcp140.dll": "msvcp140.dll",
                "msvc_runtime-14.44.35112.data/data/Scripts/msvcp140_1.dll": "msvcp140_1.dll",
                "msvc_runtime-14.44.35112.data/data/Scripts/vcruntime140.dll": "vcruntime140.dll",
                "msvc_runtime-14.44.35112.data/data/Scripts/vcruntime140_1.dll": "vcruntime140_1.dll",
            },
        ),
    ],
    "linux": [
        (
            "https://files.pythonhosted.org/packages/0d/ac/67ebbaab4b3083f2a6b27ee6c4aa400c7f8d6c72b5499aac7e4cd6ba74f5/onnxruntime-1.31.0-cp313-cp313-manylinux_2_28_x86_64.whl",
            "37fd78cee5160c7a43a1730ccb3682ffd880af9c9e80385d625c0c2f8b125809",
            {"__glob__libonnxruntime.so": "libonnxruntime.so"},
        ),
    ],
}


def fetch(url: str, sha256: str) -> bytes:
    print(f"  downloading {url.rsplit('/', 1)[-1]} ...", flush=True)
    with urllib.request.urlopen(url) as r:
        data = r.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"Invalid checksum for {url}\n expected {sha256}\n got      {digest}")
    return data


def extract(url: str, sha256: str, files: dict, dest: Path) -> None:
    if all((dest / name).exists() for name in files.values()):
        print(f"  already present: {', '.join(files.values())}")
        return
    z = zipfile.ZipFile(io.BytesIO(fetch(url, sha256)))
    names = z.namelist()
    for src, name in files.items():
        if src.startswith("__glob__"):
            stem = src[len("__glob__"):]
            matches = [n for n in names if n.rsplit("/", 1)[-1].startswith(stem)]
            if not matches:
                sys.exit(f"{stem} not found in the wheel")
            src = sorted(matches, key=len)[-1]
        (dest / name).write_bytes(z.read(src))
        print(f"  -> {dest / name}")


def main() -> None:
    ap = argparse.ArgumentParser()
    default = "windows" if platform.system() == "Windows" else "linux"
    ap.add_argument("--runtime", choices=["windows", "linux", "none"], default=default)
    args = ap.parse_args()

    MODELS.mkdir(exist_ok=True)
    print("Models:")
    for url, sha, files in MODEL_WHEELS:
        extract(url, sha, files, MODELS)
    if args.runtime != "none":
        print(f"ONNX Runtime ({args.runtime}) :")
        for url, sha, files in RUNTIME_WHEELS[args.runtime]:
            extract(url, sha, files, MODELS)
    print("OK")


if __name__ == "__main__":
    main()
