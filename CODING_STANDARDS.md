# Coding Standards

Judgement calls for review. Rules that a tool can check (formatting, lints, `mosaic-core` having no GUI dependencies) are enforced by CI and the pre-commit hook instead.

## Domain logic lives in `mosaic-core`

`mosaic-app` is a thin shell: it gathers input, calls `mosaic-core`, and draws what comes back. Scanning, Source Image identity, matching, assignment, the `.mosaic` format, inspection queries and viewport math (screen ↔ Tile) all belong in `mosaic-core`. Flag any app code that computes something a test would want to cover.

## Source Images stay unaltered

A Source Image's pixels are only ever cropped and scaled, with any transparency shown on white. This applies on every path: rendering, Thumbnails, the Original shown in the inspector and PNG export (ADR 0002). Flag any color, brightness, blend or filter operation applied to Source Image pixels.

## Glossary vocabulary

Identifiers, types, test names, UI strings and comments use the terms in `GLOSSARY.md`. `scripts/check-glossary-terms.sh` catches the synonyms listed under _Avoid_; flag any other synonym for a glossary term, and flag a new domain concept with no glossary entry as a gap.

## Tests go through the `mosaic-core` public API

Tests build fixtures (synthetic images in temp folders), call the public API (Build, Persist, Inspect, Viewport, Export) and assert on what it returns. Flag tests that reach into private modules or the internals of a Mosaic File other than through Persist. A Persist test may rewrite `format_version` in a saved Mosaic File's `manifest.json` to check that other versions are refused, but flag one that depends on any other part of the file's layout. A test may treat the analysis cache folder as opaque (overwrite or delete everything in it, as a corrupt or missing cache would), but flag one that names cache entry paths or depends on the entry format.

The one exception is `mosaic-app`'s smoke tests (`crates/mosaic-app/src/smoke_tests.rs`), which drive the window headlessly with `egui_kittest` to check it is wired up (a click pins, Escape unpins) and save a screenshot to `target/screenshots/` for PRs. They may set up the window's state and read where a Tile is drawn, but they assert only on what the window shows. Keep them thin: a smoke test may check that the window shows what `mosaic-core` returned for its fixture (a filename, a Tile count), but flag one that covers a computation's cases (other settings, edge cases, viewport math); those tests belong in `mosaic-core`.
