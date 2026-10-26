# Source Images are never tinted

Many mosaic makers blend each Source Image toward its Tile's target color to improve fidelity. We deliberately don't: a Source Image is only center-cropped and scaled, never color-altered. The purpose of the app is to identify the original artwork (and eventually its artist), and tinting misrepresents other people's work. Fidelity comes from better matching instead (each Source Image's Color Signature, in a perceptual color space). Don't add tinting as a "quick quality win". If it's ever reconsidered, it should be an explicit opt-in that never applies to inspection previews.

## Consequences

- A Source Image with transparency is shown on white: its transparent areas are filled with white before matching, and that same picture appears in rendering, thumbnails, previews and export. This isn't tinting: opaque pixels stay untouched, and white is the background such artwork is usually drawn for. Filling with the Tile's color was rejected, because a Source Image would then look different depending on where it sits.
- A transparent Target Image is shown on white the same way, so it is matched as it looks rather than by whatever colors its transparent pixels happen to store.
- Pixels are read as sRGB; embedded color profiles are ignored in v1.
