use std::ops::Range;

use crate::mosaic::{Mosaic, Tile};

/// A point on screen, in the units the app draws in, with y growing downwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
}

/// An axis-aligned rectangle on screen, from its top-left corner `min` to its bottom-right
/// corner `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub min: ScreenPoint,
    pub max: ScreenPoint,
}

/// How a Mosaic is panned and zoomed on screen: where its Tiles are drawn, and which Tile
/// sits under a point.
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    tiles_across: u32,
    rows: u32,
    /// Where the Mosaic's top-left corner is drawn.
    origin: ScreenPoint,
    /// The side of one Tile on screen.
    tile_side: f32,
}

impl Viewport {
    /// The smallest a Tile can be zoomed out to.
    pub const MIN_TILE_SIDE: f32 = 0.25;
    /// The largest a Tile can be zoomed in to.
    pub const MAX_TILE_SIDE: f32 = 1024.0;

    /// A viewport showing the whole of `mosaic` as large as fits in `screen`, centered.
    pub fn fitting(mosaic: &Mosaic, screen: ScreenRect) -> Self {
        let (tiles_across, rows) = (mosaic.tiles_across(), mosaic.rows());
        let width = screen.max.x - screen.min.x;
        let height = screen.max.y - screen.min.y;
        let tile_side = (width / tiles_across as f32)
            .min(height / rows as f32)
            .clamp(Self::MIN_TILE_SIDE, Self::MAX_TILE_SIDE);
        let origin = ScreenPoint {
            x: screen.min.x + (width - tile_side * tiles_across as f32) / 2.0,
            y: screen.min.y + (height - tile_side * rows as f32) / 2.0,
        };
        Self {
            tiles_across,
            rows,
            origin,
            tile_side,
        }
    }

    /// The side of one Tile on screen.
    pub fn tile_side(&self) -> f32 {
        self.tile_side
    }

    /// Where the whole Mosaic is drawn.
    pub fn mosaic_rect(&self) -> ScreenRect {
        ScreenRect {
            min: self.origin,
            max: self.screen_point(self.tiles_across as f32, self.rows as f32),
        }
    }

    /// The Tile under `point`, or `None` if `point` is off the Mosaic.
    pub fn tile_at(&self, point: ScreenPoint) -> Option<Tile> {
        let column = ((point.x - self.origin.x) / self.tile_side).floor();
        let row = ((point.y - self.origin.y) / self.tile_side).floor();
        let on_mosaic = (0.0..self.tiles_across as f32).contains(&column)
            && (0.0..self.rows as f32).contains(&row);
        on_mosaic.then(|| {
            let tile = Tile {
                column: column as u32,
                row: row as u32,
            };
            // Rounding can put a point on a Tile's very edge into its neighbour; settle such
            // points the same way `tile_rect` draws them.
            self.nudged_into_rect(tile, point)
        })
    }

    /// Where `tile` is drawn. Neighbouring Tiles share their edges exactly.
    pub fn tile_rect(&self, tile: Tile) -> ScreenRect {
        ScreenRect {
            min: self.screen_point(tile.column as f32, tile.row as f32),
            max: self.screen_point((tile.column + 1) as f32, (tile.row + 1) as f32),
        }
    }

    /// The Tiles drawn at least partly inside `screen`, row by row.
    pub fn visible_tiles(&self, screen: ScreenRect) -> impl Iterator<Item = Tile> + use<> {
        let (columns, rows) = (
            self.visible_range(screen.min.x, screen.max.x, self.origin.x, self.tiles_across),
            self.visible_range(screen.min.y, screen.max.y, self.origin.y, self.rows),
        );
        rows.flat_map(move |row| columns.clone().map(move |column| Tile { column, row }))
    }

    /// Zooms by `factor` (above 1 zooms in) while keeping the spot under `point` where it is.
    /// The zoom stops at [`Self::MIN_TILE_SIDE`] and [`Self::MAX_TILE_SIDE`].
    pub fn zoom_about(&mut self, point: ScreenPoint, factor: f32) {
        let tile_side = (self.tile_side * factor).clamp(Self::MIN_TILE_SIDE, Self::MAX_TILE_SIDE);
        let applied = tile_side / self.tile_side;
        self.origin = ScreenPoint {
            x: point.x - (point.x - self.origin.x) * applied,
            y: point.y - (point.y - self.origin.y) * applied,
        };
        self.tile_side = tile_side;
    }

    /// Moves the Mosaic by `dx` across and `dy` down the screen.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.origin.x += dx;
        self.origin.y += dy;
    }

    /// The screen point at `column`, `row` in Tile units from the Mosaic's top-left corner.
    fn screen_point(&self, column: f32, row: f32) -> ScreenPoint {
        ScreenPoint {
            x: self.origin.x + column * self.tile_side,
            y: self.origin.y + row * self.tile_side,
        }
    }

    /// `tile`, or the neighbour whose `tile_rect` holds `point` when `point` sits on an edge
    /// that rounding put on the other side.
    fn nudged_into_rect(&self, tile: Tile, point: ScreenPoint) -> Tile {
        let rect = self.tile_rect(tile);
        let nudge = |index: u32, coordinate: f32, min: f32, max: f32, count: u32| {
            if coordinate < min && index > 0 {
                index - 1
            } else if coordinate >= max && index + 1 < count {
                index + 1
            } else {
                index
            }
        };
        Tile {
            column: nudge(
                tile.column,
                point.x,
                rect.min.x,
                rect.max.x,
                self.tiles_across,
            ),
            row: nudge(tile.row, point.y, rect.min.y, rect.max.y, self.rows),
        }
    }

    /// The Tile columns (or rows) drawn at least partly between `min` and `max` on one screen
    /// axis, for a Mosaic starting at `origin` and `count` Tiles long.
    fn visible_range(&self, min: f32, max: f32, origin: f32, count: u32) -> Range<u32> {
        let index = |position: f32| (position / self.tile_side).clamp(0.0, count as f32);
        index(min - origin).floor() as u32..index(max - origin).ceil() as u32
    }
}
