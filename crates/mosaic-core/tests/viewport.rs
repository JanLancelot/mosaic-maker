mod common;

use common::{Fixture, RED, solid};
use mosaic_core::{Mosaic, ScreenPoint, ScreenRect, Tile, Viewport};

/// A Mosaic 4 Tiles Across by 3 rows.
fn four_by_three() -> Mosaic {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(40, 30, RED));
    fixture.build(&target_image, 4)
}

fn point(x: f32, y: f32) -> ScreenPoint {
    ScreenPoint { x, y }
}

fn screen() -> ScreenRect {
    ScreenRect {
        min: point(100.0, 50.0),
        max: point(900.0, 650.0),
    }
}

/// Every Tile of `mosaic`, row by row.
fn tiles(mosaic: &Mosaic) -> Vec<Tile> {
    (0..mosaic.rows())
        .flat_map(|row| (0..mosaic.tiles_across()).map(move |column| Tile { column, row }))
        .collect()
}

fn center(rect: ScreenRect) -> ScreenPoint {
    point(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    )
}

/// Asserts that each Tile's screen rectangle maps back to that Tile, and that points across
/// the Mosaic map to a Tile whose rectangle holds them.
fn assert_inverse(mosaic: &Mosaic, viewport: &Viewport) {
    for tile in tiles(mosaic) {
        let rect = viewport.tile_rect(tile);
        assert!(
            rect.max.x > rect.min.x && rect.max.y > rect.min.y,
            "{rect:?}"
        );
        assert_eq!(viewport.tile_at(center(rect)), Some(tile), "{rect:?}");
        assert_eq!(viewport.tile_at(rect.min), Some(tile), "{rect:?}");
    }
    let mosaic_rect = viewport.mosaic_rect();
    for i in 0..20 {
        for j in 0..20 {
            let p = point(
                mosaic_rect.min.x
                    + (mosaic_rect.max.x - mosaic_rect.min.x) * (i as f32 + 0.37) / 20.0,
                mosaic_rect.min.y
                    + (mosaic_rect.max.y - mosaic_rect.min.y) * (j as f32 + 0.61) / 20.0,
            );
            let tile = viewport
                .tile_at(p)
                .expect("a point on the Mosaic is on a Tile");
            let rect = viewport.tile_rect(tile);
            assert!(
                (rect.min.x..rect.max.x).contains(&p.x) && (rect.min.y..rect.max.y).contains(&p.y),
                "{p:?} maps to {tile:?} at {rect:?}"
            );
        }
    }
}

#[test]
fn a_fitted_viewport_shows_the_whole_mosaic_centered_with_square_tiles() {
    let mosaic = four_by_three();

    let viewport = Viewport::fitting(&mosaic, screen());

    // 800 × 600 holds 4 × 3 Tiles of 200 px exactly.
    assert_eq!(viewport.tile_side(), 200.0);
    assert_eq!(viewport.mosaic_rect(), screen());
    assert_eq!(
        viewport.tile_rect(Tile { column: 1, row: 2 }),
        ScreenRect {
            min: point(300.0, 450.0),
            max: point(500.0, 650.0),
        }
    );
}

#[test]
fn screen_to_tile_and_tile_to_screen_are_inverse_at_various_pan_and_zoom_levels() {
    let mosaic = four_by_three();
    let mut viewport = Viewport::fitting(&mosaic, screen());
    assert_inverse(&mosaic, &viewport);

    viewport.zoom_about(point(321.0, 123.0), 3.7);
    assert_inverse(&mosaic, &viewport);
    viewport.pan(-57.5, 230.25);
    assert_inverse(&mosaic, &viewport);
    viewport.zoom_about(point(800.0, 600.0), 0.13);
    assert_inverse(&mosaic, &viewport);
    viewport.pan(1234.0, -987.0);
    assert_inverse(&mosaic, &viewport);
}

#[test]
fn points_outside_the_mosaic_are_on_no_tile() {
    let mosaic = four_by_three();
    let mut viewport = Viewport::fitting(&mosaic, screen());
    viewport.zoom_about(point(400.0, 300.0), 0.5);
    let ScreenRect { min, max } = viewport.mosaic_rect();
    let middle = center(viewport.mosaic_rect());

    for outside in [
        point(min.x - 0.5, middle.y),
        point(middle.x, min.y - 0.5),
        point(max.x, middle.y),
        point(middle.x, max.y),
        point(max.x + 100.0, max.y + 100.0),
        point(-1.0e6, -1.0e6),
    ] {
        assert_eq!(viewport.tile_at(outside), None, "{outside:?}");
    }
    assert_eq!(viewport.tile_at(min), Some(Tile { column: 0, row: 0 }));
}

/// Where `p` sits on the Mosaic: its Tile and how far across and down that Tile it is.
fn spot_under(viewport: &Viewport, p: ScreenPoint) -> (Tile, f32, f32) {
    let tile = viewport.tile_at(p).unwrap();
    let rect = viewport.tile_rect(tile);
    let side = rect.max.x - rect.min.x;
    (tile, (p.x - rect.min.x) / side, (p.y - rect.min.y) / side)
}

fn assert_same_spot(before: (Tile, f32, f32), after: (Tile, f32, f32)) {
    assert_eq!(before.0, after.0);
    assert!((before.1 - after.1).abs() < 1e-3, "{before:?} vs {after:?}");
    assert!((before.2 - after.2).abs() < 1e-3, "{before:?} vs {after:?}");
}

#[test]
fn zooming_about_a_point_keeps_that_point_fixed() {
    let mosaic = four_by_three();
    let mut viewport = Viewport::fitting(&mosaic, screen());
    let cursor = point(437.0, 311.0);

    for factor in [2.5, 1.1, 0.3, 4.0] {
        let before = spot_under(&viewport, cursor);
        let tile_side = viewport.tile_side();
        viewport.zoom_about(cursor, factor);
        assert!((viewport.tile_side() - tile_side * factor).abs() < 1e-3);
        assert_same_spot(before, spot_under(&viewport, cursor));
    }
}

#[test]
fn zoom_stops_at_its_limits_and_still_keeps_the_point_fixed() {
    let mosaic = four_by_three();
    let mut viewport = Viewport::fitting(&mosaic, screen());
    let cursor = point(437.0, 311.0);

    let before = spot_under(&viewport, cursor);
    viewport.zoom_about(cursor, 1.0e6);
    assert_eq!(viewport.tile_side(), Viewport::MAX_TILE_SIDE);
    assert_same_spot(before, spot_under(&viewport, cursor));

    viewport.zoom_about(cursor, 1.0e-9);
    assert_eq!(viewport.tile_side(), Viewport::MIN_TILE_SIDE);
}

#[test]
fn visible_tiles_are_exactly_those_drawn_at_least_partly_on_screen() {
    let mosaic = four_by_three();
    let mut viewport = Viewport::fitting(&mosaic, screen());
    let small_screen = ScreenRect {
        min: point(250.0, 120.0),
        max: point(610.0, 410.0),
    };

    for (zoom, dx, dy) in [(1.0, 0.0, 0.0), (2.3, -40.0, 75.0), (0.4, 300.0, -10.0)] {
        viewport.zoom_about(point(500.0, 300.0), zoom);
        viewport.pan(dx, dy);
        let expected: Vec<Tile> = tiles(&mosaic)
            .into_iter()
            .filter(|&tile| {
                let rect = viewport.tile_rect(tile);
                rect.min.x < small_screen.max.x
                    && small_screen.min.x < rect.max.x
                    && rect.min.y < small_screen.max.y
                    && small_screen.min.y < rect.max.y
            })
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(
            viewport.visible_tiles(small_screen).collect::<Vec<_>>(),
            expected
        );
    }

    viewport.pan(5000.0, 0.0);
    assert_eq!(viewport.visible_tiles(small_screen).count(), 0);
}

#[test]
fn a_tile_that_only_touches_the_screen_edge_is_not_visible() {
    let mosaic = four_by_three();
    let viewport = Viewport::fitting(&mosaic, screen());
    // Exactly the 200 px Tiles in columns 1 and 2 of row 1.
    let edges = ScreenRect {
        min: point(300.0, 250.0),
        max: point(700.0, 450.0),
    };

    assert_eq!(
        viewport.visible_tiles(edges).collect::<Vec<_>>(),
        [Tile { column: 1, row: 1 }, Tile { column: 2, row: 1 }]
    );
}
