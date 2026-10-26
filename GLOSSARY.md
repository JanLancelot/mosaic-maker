# Mosaic Maker

Recreates a picture out of many smaller pictures, and lets you inspect which picture sits where.

## Language

**Target Image**:
The picture the Mosaic recreates.
_Avoid_: Input image, base image

**Source Image**:
One picture from the Source Library that can fill Tiles in a Mosaic. It is identified by its contents, not its file location, so the same picture stored twice is one Source Image. It always appears unaltered (cropped, never tinted).
_Avoid_: Reference image, tile image, photo

**Original**:
The file holding a Source Image, at the place in the Source Library where it was last found. Once that file is moved, renamed, edited or deleted, the Original is unreachable.
_Avoid_: Full-resolution image

**Thumbnail**:
A small square copy of a Source Image, center-cropped as it appears in its Tiles, kept with the Mosaic so the Source Image can still be shown when its Original is unreachable.
_Avoid_: Preview, icon

**Source Library**:
The folder of Source Images a Mosaic is built from, including its subfolders. Hidden files and folders are not part of it.
_Avoid_: Reference folder, image pool

**Tile**:
One square grid position in a Mosaic. A Tile is a position, never the picture that fills it.
_Avoid_: Cell, slot

**Color Signature**:
The color layout of a Source Image, or of the Target Image within one Tile: which colors sit where across it. A Tile is filled by a Source Image whose Color Signature is close to its own.
_Avoid_: Fingerprint, hash, descriptor, average color

**Mosaic**:
A grid of Tiles that recreates a Target Image, where each Tile is filled by one Source Image. It persists, so it can be reopened and inspected later.
_Avoid_: Output, result, collage

**Repeat Radius**:
The distance in Tiles within which the same Source Image should not fill two Tiles.
_Avoid_: Spacing, cooldown

**Tiles Across**:
The number of Tile columns in a Mosaic. It sets how fine the Mosaic is, and the row count follows from the Target Image's shape.
_Avoid_: Resolution, grid size

**Seed**:
The number that decides which of the equally valid Mosaics a build makes. The same Target Image, Source Library, Tiles Across, Repeat Radius and Seed always give the same Mosaic.
_Avoid_: Variation
