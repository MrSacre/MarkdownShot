# Calibration pages (lorem ipsum)

Synthetic pages that keep alternating pictures, lists, code blocks and text, with the cases known to be
hard for the engine: text between two close pictures, a list right under a picture, code right under a
list, two pictures side by side, sparse line-art diagrams, code screenshots narrower than the column (with
a full-screen button), an annotated dialog, highlighted inline code, a console with aligned columns
(addresses such as `00007ff6`1a2b3c4a`), nested lists with `####` subheadings, small icons, bordered tables
with inline code expected as Markdown tables (at the end of a page, right under a list, right under two
pictures side by side), and the picture of a comparison table on a tinted background.

`table.png` (with `table.md`, the expected Markdown, and `table.json`, its frame) is used by the
`bordered_table_to_markdown` test; `table-checks.png` and `table-frame.png` (drawn tables that must stay
pictures) by `drawn_tables_stay_images`. `harness/eval.py` scores the table cells (the "cells" column of
`harness/calibration.py`).

Each page exists in a light and a dark theme, at 100% and 125%.

```bash
python testdata/calibration/make_assets.py   # pictures (already versioned)
python testdata/calibration/make_pages.py    # doc.html, tuto.html, notes.html (already versioned)
python testdata/calibration/capture.py       # out/<page>-<theme>-<scale>/: full page, scroll frames,
                                             # compressed video, expected.md, figures.json
cargo build --release
python harness/calibration.py --save before.json        # per-page scores (scroll frames)
python harness/calibration.py --compare before.json     # before / after table
python harness/calibration.py --source frames_video     # same pages, compressed video
python harness/calibration.py --only tuto-light-100 --show   # block by block details
```

Only `expected.md` (expected Markdown, extracted from the DOM) and `figures.json` (exact picture
rectangles, `getBoundingClientRect` times the scale) are versioned; the captures are regenerated.
