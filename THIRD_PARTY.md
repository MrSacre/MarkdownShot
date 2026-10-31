# Third-party components

MarkdownShot (MIT license) relies on the following components, shipped with the application or downloaded
by `scripts/fetch_models.py`.

| Component | Use | License | Source |
|---|---|---|---|
| PP-OCRv6 models (detection and recognition, "small" and "tiny") | OCR | Apache-2.0 | PaddleOCR, through the PyPI packages `rapidocr` 3.10.0 and `onnxocr` 4.0.0 |
| PP-Layout CDLA model (PicoDet) | Layout analysis | Apache-2.0 | PaddleOCR, through the PyPI package `rapid_layout` 1.2.1 |
| ONNX Runtime 1.31 (Linux) and 1.24 with DirectML (Windows) | Model inference | MIT | Microsoft, through the PyPI packages `onnxruntime` and `onnxruntime-directml` |
| DirectML (DirectML.dll) | Optional GPU inference on Windows | MIT | Microsoft, through the PyPI package `onnxruntime-directml` |
| libwebp (through `libwebp-sys`) | WebP images | BSD-3-Clause | Google |
| Visual C++ runtime (msvcp140, vcruntime140) | ONNX Runtime dependency on Windows | Microsoft Visual C++ redistributable license | through the PyPI package `msvc-runtime` |
| Liberation Sans Bold | Interface font | SIL Open Font License 1.1 | Red Hat, see `assets/fonts/LICENSE-LiberationSans.txt` |

The Rust libraries are listed in `Cargo.lock` (mostly MIT or Apache-2.0), among them `ort`, `image`,
`ndarray`, `winit`, `egui`, `softbuffer`, `tray-icon`, `global-hotkey`, `xcap`, `arboard`, `rfd`, `fontdue`,
`serde`, `toml`, `directories`, `chrono`, `windows-sys` and `embed-resource`.
