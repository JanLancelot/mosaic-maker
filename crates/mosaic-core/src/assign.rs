use crate::build::BuildError;
use crate::color_signature::ColorSignature;
use crate::mosaic::Tile;
use crate::warning::BuildWarning;

/// Which Source Image fills each Tile, and the Tiles where the Repeat Radius had to shrink.
pub(crate) struct Assignment {
    /// Row-major: the index into the Source Images filling each Tile.
    pub(crate) tile_source_images: Vec<usize>,
    pub(crate) warnings: Vec<BuildWarning>,
}

/// Fills each Tile with the Source Image whose Color Signature is closest to the Tile's,
/// among those not already filling a Tile within `repeat_radius` (Chebyshev distance).
///
/// Tiles are visited in a random order drawn from `seed`, so no part of the Mosaic always
/// gets first pick. When every Source Image is already used within the Repeat Radius, it
/// shrinks one Tile at a time for that Tile alone, and a warning records the radius used.
/// `tile_color_signatures` is row-major; `on_tile` hears how many Tiles are filled so far
/// and can stop the assignment by returning an error. Ties go to the earliest Source Image.
pub(crate) fn assign(
    tiles_across: u32,
    tile_color_signatures: &[ColorSignature],
    source_image_color_signatures: &[ColorSignature],
    repeat_radius: u32,
    seed: u64,
    mut on_tile: impl FnMut(usize) -> Result<(), BuildError>,
) -> Result<Assignment, BuildError> {
    let tile_count = tile_color_signatures.len();
    let rows = tile_count as u32 / tiles_across;
    let source_image_count = source_image_color_signatures.len();
    let mut tile_source_images: Vec<Option<usize>> = vec![None; tile_count];
    let mut warnings = Vec::new();

    // For the Tile being filled: the Chebyshev distance to the nearest Tile each Source
    // Image already fills, valid only where `seen_on_visit` matches the current visit.
    let mut nearest = vec![0u32; source_image_count];
    let mut seen_on_visit = vec![usize::MAX; source_image_count];
    // How many Source Images are nearest at each distance, 0 to `repeat_radius`.
    let mut nearest_counts = vec![0usize; repeat_radius as usize + 1];

    for (visit, tile_index) in visiting_order(tile_count, seed).into_iter().enumerate() {
        let tile = Tile {
            column: tile_index as u32 % tiles_across,
            row: tile_index as u32 / tiles_across,
        };
        nearest_counts.fill(0);
        for neighbour in tiles_within(tile, repeat_radius, tiles_across, rows) {
            let neighbour_index = (neighbour.row * tiles_across + neighbour.column) as usize;
            let Some(source_image) = tile_source_images[neighbour_index] else {
                continue;
            };
            let distance = tile
                .column
                .abs_diff(neighbour.column)
                .max(tile.row.abs_diff(neighbour.row));
            if seen_on_visit[source_image] != visit {
                seen_on_visit[source_image] = visit;
                nearest[source_image] = distance;
                nearest_counts[distance as usize] += 1;
            } else if distance < nearest[source_image] {
                nearest_counts[nearest[source_image] as usize] -= 1;
                nearest[source_image] = distance;
                nearest_counts[distance as usize] += 1;
            }
        }

        // The widest radius that still leaves some Source Image free. Nothing is nearer
        // than 1, so a radius of 0 always does.
        let mut radius = 0;
        let mut blocked = 0;
        for distance in 1..=repeat_radius {
            blocked += nearest_counts[distance as usize];
            if blocked == source_image_count {
                break;
            }
            radius = distance;
        }
        if radius < repeat_radius {
            warnings.push(BuildWarning::RepeatRadiusRelaxed {
                tile,
                repeat_radius: radius,
            });
        }

        let tile_color_signature = &tile_color_signatures[tile_index];
        let chosen = source_image_color_signatures
            .iter()
            .enumerate()
            .filter(|&(source_image, _)| {
                seen_on_visit[source_image] != visit || nearest[source_image] > radius
            })
            .map(|(source_image, color_signature)| {
                (source_image, color_signature.distance(tile_color_signature))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(source_image, _)| source_image)
            .expect("a radius of 0 leaves every Source Image free");
        tile_source_images[tile_index] = Some(chosen);
        on_tile(visit + 1)?;
    }

    Ok(Assignment {
        tile_source_images: tile_source_images
            .into_iter()
            .map(|source_image| source_image.expect("every Tile is visited"))
            .collect(),
        warnings,
    })
}

/// The Tiles other than `tile` within `radius` of it, clipped to the grid.
fn tiles_within(
    tile: Tile,
    radius: u32,
    tiles_across: u32,
    rows: u32,
) -> impl Iterator<Item = Tile> {
    let columns = tile.column.saturating_sub(radius)..=(tile.column + radius).min(tiles_across - 1);
    let row_range = tile.row.saturating_sub(radius)..=(tile.row + radius).min(rows - 1);
    row_range
        .flat_map(move |row| columns.clone().map(move |column| Tile { column, row }))
        .filter(move |&neighbour| neighbour != tile)
}

/// The Tile indices `0..tile_count` shuffled by `seed`. The shuffle is written out here,
/// rather than taken from a library, so a seed gives the same order in every version.
fn visiting_order(tile_count: usize, seed: u64) -> Vec<usize> {
    let mut random = SplitMix64(seed);
    let mut order: Vec<usize> = (0..tile_count).collect();
    for i in (1..tile_count).rev() {
        order.swap(i, random.below(i as u64 + 1) as usize);
    }
    order
}

/// A small, fast pseudo-random number generator (Steele, Lea & Flood's SplitMix64).
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`, nearly uniform for any `bound` far below 2⁶⁴.
    fn below(&mut self, bound: u64) -> u64 {
        ((u128::from(self.next()) * u128::from(bound)) >> 64) as u64
    }
}
