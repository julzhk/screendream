use std::collections::HashSet;

use screendream::render::fractal_curves::{
    CurveDrawer, CurveKind, DOMAIN_X, DOMAIN_Y, curve_points,
};

#[test]
fn hilbert_curve_visits_every_grid_cell_once() {
    for order in 1..=4 {
        let points = curve_points(CurveKind::Hilbert, order);
        let side = 1usize << order;
        assert_eq!(points.len(), side * side, "order {order}");
        let mut seen = HashSet::new();
        for p in &points {
            assert!(seen.insert((p[0] as i32, p[1] as i32)), "revisited {p:?}");
        }
        for pair in points.windows(2) {
            let step = (pair[1][0] - pair[0][0]).abs() + (pair[1][1] - pair[0][1]).abs();
            assert!((step - 1.0).abs() < 1e-3, "non-unit step");
        }
    }
}

#[test]
fn peano_curve_fills_a_power_of_three_grid() {
    let points = curve_points(CurveKind::Peano, 2);
    assert_eq!(points.len(), 81);
    let distinct: HashSet<_> = points.iter().map(|p| (p[0] as i32, p[1] as i32)).collect();
    assert_eq!(distinct.len(), 81);
}

#[test]
fn moore_curve_is_a_closed_loop() {
    let points = curve_points(CurveKind::Moore, 2);
    let (first, last) = (points[0], points[points.len() - 1]);
    let gap = (first[0] - last[0]).abs() + (first[1] - last[1]).abs();
    assert!(
        (gap - 1.0).abs() < 1e-3,
        "ends should be adjacent, gap {gap}"
    );
}

#[test]
fn louder_bands_draw_faster() {
    let mut drawer = CurveDrawer::new(2, 8192, 42);
    // Each pen rewrites its growing head segment every frame, so count distinct slots.
    let mut slots = [HashSet::new(), HashSet::new()];
    for frame in 0..600 {
        for write in drawer.advance(frame as f32 / 60.0, 1.0 / 60.0, &[1.0, 0.0]) {
            slots[write.segment.pen as usize].insert(write.slot);
        }
    }
    assert!(
        slots[0].len() > slots[1].len() * 5,
        "loud pen {} vs quiet pen {}",
        slots[0].len(),
        slots[1].len()
    );
}

#[test]
fn traces_stay_on_screen_and_coverage_grows() {
    let mut drawer = CurveDrawer::new(8, 16384, 7);
    let levels = [0.8; 8];
    for frame in 0..1800 {
        for write in drawer.advance(frame as f32 / 60.0, 1.0 / 60.0, &levels) {
            for p in [write.segment.a, write.segment.b] {
                assert!(
                    p[0].abs() <= DOMAIN_X + 1e-4 && p[1].abs() <= DOMAIN_Y + 1e-4,
                    "{p:?}"
                );
            }
            assert!(write.slot < 16384);
        }
    }
    assert_eq!(drawer.heads().count(), 8);
    assert!(drawer.coverage() > 0.2, "coverage {}", drawer.coverage());
}
