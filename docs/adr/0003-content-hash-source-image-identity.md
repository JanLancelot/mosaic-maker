# Source Images are identified by content hash

A Source Image's identity is a hash of its file contents. Its path relative to the Source Library is stored only as a last-known location for finding the file again. Saved Mosaics, the analysis cache and the planned artist/link metadata all key on this identity, so all three survive renames, moves and library reorganisation. The same picture stored twice counts as one Source Image.

## Consequences

- Building a Mosaic requires reading and hashing every file. Caching keyed by hash keeps re-runs cheap.
- Editing an image file (even just its metadata) makes it a new Source Image. Saved Mosaics still show the old one from their embedded thumbnail.
