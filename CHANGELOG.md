# Changelog

## 1.2.0

Accuracy of code and inline code.

- Code blocks are now read a second time by a small glyph classifier that looks at each cell of the
  monospace grid. It corrects the symbols the text recognizer gets wrong (backticks, `^`, `|`, quotes,
  brackets, dashes), removes characters invented on empty cells, and reads the short lines the recognizer
  skipped (a lone `}` or `{`, the `|` and `^^^` lines of compiler output).
- Lines of a code block that the recognizer merged or misaligned are read again row by row on the grid.
- The pitch of the monospace grid is measured more reliably on short blocks.
- Inline code on a faint background (GitHub, MkDocs, Docusaurus style chips) is detected much more often.
- A code panel with a visible horizontal scrollbar is no longer taken for a picture.
- Benchmarks: `harness/realbench.py` can render pages like Windows does (LCD subpixel text, other fonts)
  and scores symbols and inline code; `harness/blockbench.py` scores code blocks alone;
  `harness/glyphnet` generates the training data and trains the glyph classifier.

## 1.1.1

- Fix: the capture shortcut no longer switches the mode. With the default `Ctrl+Alt+S`, the `S` of the
  shortcut reached the capture toolbar and toggled Snipe, so a capture started in Snipe mode ran in Auto.
  Letter keys of the toolbar now ignore keys held with `Ctrl`, `Alt` or `Win` and the first 0.4 s.
- The portable version updates itself too: one click on the notification downloads the new zip,
  replaces the files in the application folder (settings kept) and restarts MarkdownShot.

## 1.1.0

- A splash screen with the logo shows while the models load at startup.
- In the review window, the Markdown preview follows the part of the page in view.
- Notes no longer start with a YAML front matter block; the `front_matter` and `tags` settings are gone.

## 1.0.0

First public release.
