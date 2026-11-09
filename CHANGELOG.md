# Changelog

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
