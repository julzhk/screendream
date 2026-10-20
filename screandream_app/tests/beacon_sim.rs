use screendream::render::beacon_sim::{
    BANDS, BeaconSim, Drive, FOCAL, MAX_BEAMS, MAX_SHAPES, SOLIDS, Solid, face_radius, faces,
    rotate_q,
};

const DT: f32 = 1.0 / 60.0;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

#[test]
fn the_platonic_solids_have_the_right_faces() {
    let expected = [
        (Solid::Tetrahedron, 4, 3),
        (Solid::Cube, 6, 4),
        (Solid::Octahedron, 8, 3),
        (Solid::Dodecahedron, 12, 5),
        (Solid::Icosahedron, 20, 3),
    ];
    for (solid, count, sides) in expected {
        let f = faces(solid);
        assert_eq!(f.len(), count, "{solid:?}");
        for face in &f {
            assert_eq!(face.corners.len(), sides, "{solid:?}");
            assert!((len(face.normal) - 1.0).abs() < 1e-4);
            // Corners lie on the unit sphere, in the face's plane, wound anticlockwise.
            for c in &face.corners {
                assert!((len(*c) - 1.0).abs() < 1e-4);
                assert!((dot(*c, face.normal) - dot(face.center, face.normal)).abs() < 1e-4);
            }
            let (a, b, c) = (face.corners[0], face.corners[1], face.corners[2]);
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            assert!(dot(n, face.normal) > 0.0, "{solid:?} face wound clockwise");
        }
    }
    assert_eq!(SOLIDS.len(), 5);
}

#[test]
fn quaternion_rotation_keeps_lengths() {
    let q = [0.3, -0.5, 0.2, 0.787];
    let n = (q.iter().map(|c| c * c).sum::<f32>()).sqrt();
    let q = q.map(|c| c / n);
    let v = rotate_q(q, [0.2, 0.7, -0.4]);
    assert!((len(v) - len([0.2, 0.7, -0.4])).abs() < 1e-5);
}

#[test]
fn beams_leave_from_faces_and_crystals_split_light() {
    let sim = BeaconSim::new(3);
    assert!(!sim.beams().is_empty() && sim.beams().len() <= MAX_BEAMS);
    for b in sim.beams() {
        assert!((len(b.dir) - 1.0).abs() < 1e-4);
        assert!(b.spread > 0.0 && b.intensity >= 0.0);
    }
    assert!(sim.shapes().iter().any(|s| s.crystal));
    // A crystal's beams come in fans of five colours from one point.
    for s in sim.shapes().iter().filter(|s| s.crystal && s.fade > 0.0) {
        let from_crystal: Vec<_> = sim
            .beams()
            .iter()
            .filter(|b| {
                len([
                    b.origin[0] - s.pos[0],
                    b.origin[1] - s.pos[1],
                    b.origin[2] - s.pos[2],
                ]) <= s.scale + 1e-3
            })
            .collect();
        assert!(
            from_crystal.len() >= 5,
            "crystal made {} beams",
            from_crystal.len()
        );
    }
}

fn spin_after(level: f32, seconds: usize) -> f32 {
    let mut sim = BeaconSim::new(5);
    for _ in 0..60 * seconds {
        sim.step(
            DT,
            Drive {
                bands: [level; BANDS],
                energy: level,
            },
        );
    }
    sim.shapes().iter().map(|s| s.spin.abs()).sum::<f32>() / MAX_SHAPES as f32
}

#[test]
fn louder_music_spins_the_shapes_faster() {
    let calm = spin_after(0.05, 3);
    let loud = spin_after(0.9, 3);
    assert!(loud > calm * 3.0, "calm {calm}, loud {loud}");
}

#[test]
fn onsets_reverse_some_spins() {
    let mut sim = BeaconSim::new(6);
    for frame in 0..60 * 20 {
        let mut bands = [0.1; BANDS];
        for (b, level) in bands.iter_mut().enumerate() {
            if frame % [30, 24, 18, 12][b] < 2 {
                *level = 0.9;
            }
        }
        sim.step(DT, Drive { bands, energy: 0.6 });
    }
    assert!(sim.reversals() > 10, "only {} reversals", sim.reversals());
}

#[test]
fn shapes_slide_through_the_view_and_are_replaced() {
    let mut sim = BeaconSim::new(7);
    let mut seen_inside = 0;
    let mut births = 0;
    let mut last_ages: Vec<f32> = sim.shapes().iter().map(|s| s.age).collect();
    for _ in 0..60 * 90 {
        sim.step(
            DT,
            Drive {
                bands: [0.4; BANDS],
                energy: 0.5,
            },
        );
        let cam = sim.camera();
        for (s, last) in sim.shapes().iter().zip(&mut last_ages) {
            if s.age < *last {
                births += 1;
            }
            *last = s.age;
            let rel = [
                s.pos[0] - cam.pos[0],
                s.pos[1] - cam.pos[1],
                s.pos[2] - cam.pos[2],
            ];
            let ahead = dot(rel, cam.forward);
            let x = dot(rel, cam.right) * FOCAL / ahead.max(0.1);
            let y = dot(rel, cam.up) * FOCAL / ahead.max(0.1);
            if ahead > 1.0 && x.abs() < 1.6 && y.abs() < 0.9 {
                seen_inside += 1;
            }
            // Out of sight at either end of their path, so they never pop in or out.
            if s.fade < 0.01 {
                assert!(ahead < 1.0 || x.abs() > 1.2 || y.abs() > 0.7 || ahead > 25.0);
            }
        }
    }
    assert!(births >= 5, "only {births} new shapes");
    assert!(seen_inside > 60 * 90, "shapes rarely in view");
    assert_eq!(sim.shapes().len(), MAX_SHAPES);
}

#[test]
fn when_beams_run_short_the_nearest_shapes_keep_theirs() {
    let mut sim = BeaconSim::new(9);
    for _ in 0..60 * 10 {
        sim.step(
            DT,
            Drive {
                bands: [0.5; BANDS],
                energy: 0.5,
            },
        );
    }
    let cam = sim.camera().pos;
    let dist = |p: [f32; 3]| len([p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]]);
    let mut shapes: Vec<_> = sim.shapes().iter().filter(|s| s.fade > 0.0).collect();
    shapes.sort_by(|a, b| dist(a.pos).total_cmp(&dist(b.pos)));
    let nearest = shapes[0];
    let lit = sim.beams().iter().any(|b| {
        len([
            b.origin[0] - nearest.pos[0],
            b.origin[1] - nearest.pos[1],
            b.origin[2] - nearest.pos[2],
        ]) <= nearest.scale + 1e-3
    });
    assert!(lit, "the nearest shape lost its beams");
}

#[test]
fn beams_pour_from_whole_faces_or_from_a_crystal_heart() {
    let gap = |p: [f32; 3], q: [f32; 3]| len([p[0] - q[0], p[1] - q[1], p[2] - q[2]]);
    for seed in 1..=4 {
        let sim = BeaconSim::new(seed);
        let mut from_hearts = 0;
        for b in sim.beams() {
            // Every beam belongs to some shape: a crystal's heart, a laser face, or a face
            // whose whole width it fills.
            let owner = sim.shapes().iter().find(|s| {
                if s.crystal {
                    return gap(b.origin, s.pos) < 1e-4;
                }
                let face = &sim.faces(s.solid)[0];
                let on_face = (gap(b.origin, s.pos) - len(face.center) * s.scale).abs() < 1e-3;
                let width = if s.laser {
                    0.02
                } else {
                    face_radius(face) * s.scale
                };
                on_face && (b.aperture - width).abs() < 1e-3
            });
            let owner = owner.expect("a beam that leaves neither a face nor a crystal's heart");
            from_hearts += owner.crystal as usize;
        }
        assert!(from_hearts >= 5, "seed {seed}: no crystal light");
    }
}

#[test]
fn beams_range_from_tight_spots_to_broad_floods() {
    let mut spreads = Vec::new();
    for seed in 1..=4 {
        let sim = BeaconSim::new(seed);
        spreads.extend(sim.beams().iter().map(|b| b.spread.atan()));
    }
    let tight = spreads.iter().filter(|&&a| a < 0.08).count();
    let wide = spreads.iter().filter(|&&a| a > 0.28).count();
    assert!(
        tight > 10 && wide > 10,
        "tight {tight}, wide {wide} of {}",
        spreads.len()
    );
}

#[test]
fn beam_brightness_changes_gradually_with_the_music() {
    let mut sim = BeaconSim::new(10);
    // Brightness and age of each shape; a shape that got younger was just replaced.
    let brightness = |sim: &BeaconSim| {
        sim.shapes()
            .iter()
            .map(|s| (s.intensity, s.age))
            .collect::<Vec<_>>()
    };
    let mean = |v: &[(f32, f32)]| v.iter().map(|x| x.0).sum::<f32>() / v.len() as f32;
    let mut last = brightness(&sim);
    let mut quiet_mean = 0.0;
    // Silence, then a sudden loud passage with hard onsets, then silence again.
    for frame in 0..60 * 15 {
        let loud = (60 * 5..60 * 10).contains(&frame);
        let level = if loud {
            if frame % 20 < 2 { 1.0 } else { 0.8 }
        } else {
            0.0
        };
        sim.step(
            DT,
            Drive {
                bands: [level; BANDS],
                energy: level,
            },
        );
        let now = brightness(&sim);
        for (&(a, age), &(b, last_age)) in now.iter().zip(&last) {
            if age >= last_age {
                assert!((a - b).abs() < 0.06, "brightness jumped from {b} to {a}");
            }
        }
        last = now;
        if frame == 60 * 5 - 1 {
            quiet_mean = mean(&last);
        }
        if frame == 60 * 10 - 1 {
            let loud_mean = mean(&last);
            assert!(
                loud_mean > quiet_mean * 2.5,
                "quiet {quiet_mean}, loud {loud_mean}"
            );
        }
    }
    // Still fading a second after the music stops, rather than snapping off.
    let end = mean(&last);
    assert!(end > quiet_mean, "already back to {end}");
}
