# Coding Standards

Judgement calls for review. Rules that a tool can check (formatting, lints, `mosaic-core` having no GUI dependencies) are enforced by CI and the pre-commit hook instead.

## Domain logic lives in `mosaic-core`

`mosaic-app` is a thin shell: it gathers input, calls `mosaic-core`, and draws what comes back. Scanning, Source Image identity, matching, assignment, the `.mosaic` format, inspection queries and viewport math (screen ↔ Tile) all belong in `mosaic-core`. Flag any app code that computes something a test would want to cover.

## Source Images stay unaltered

A Source Image's pixels are only ever center-cropped and scaled. This applies on every path: rendering, thumbnails, inspector previews and PNG export (ADR 0002). Flag any color, brightness, blend or filter operation applied to Source Image pixels.

## Glossary vocabulary

Identifiers, types, test names, UI strings and comments use the terms in `GLOSSARY.md`. `scripts/check-glossary-terms.sh` catches the synonyms listed under _Avoid_; flag any other synonym for a glossary term, and flag a new domain concept with no glossary entry as a gap.

## Tests go through the `mosaic-core` public API

Tests build fixtures (synthetic images in temp folders), call the public API (Build, Persist, Inspect, Viewport, Export) and assert on what it returns. Flag tests that reach into private modules or the internals of the `.mosaic` file other than through Persist. A test may treat the analysis cache folder as opaque (overwrite or delete everything in it, as a corrupt or missing cache would), but flag one that names cache entry paths or depends on the entry format.
