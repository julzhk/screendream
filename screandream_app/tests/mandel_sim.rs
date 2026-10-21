use screendream::analysis::audio_features::SPECTRUM_BINS;
use screendream::render::mandel_sim::{
    BANDS, DISSOLVE, Dive, Drive, MAX_DIVE, MAX_PALETTE_RANGE, MAX_SEGMENTS, MESA_HEIGHT, MIN_SPAN,
    MandelSim, PEAK_HEIGHT, Palette, START_SPAN, Set, band_of, escape, find_target, height,
    iterations,
};
use screendream::render::sim_math::Rng;

const DT: f32 = 1.0 / 60.0;

fn loud() -> Drive {
    Drive {
        bands: [0.8; BANDS],
        energy: 0.8,
        spectrum: [0.7; SPECTRUM_BINS],
    }
}

fn dive(set: Set, centre: [f64; 2], span: f64) -> Dive {
    Dive {
        set,
        centre,
        span,
        rot: 0.7,
        spin: 0.0,
        phase: 0.0,
        palette: Palette {
            hue: 0.0,
            range: 0.2,
            saturation: 1.0,
        },
        age: 0.0,
        id: 0,
        wobble: [0.0; 2],
    }
}

#[test]
fn points_in_the_set_never_escape_and_points_outside_do() {
    for p in [[0.0, 0.0], [-1.0, 0.0], [-0.12, 0.75], [-1.75, 0.0]] {
        assert!(escape(Set::Mandelbrot, p, 500).is_none(), "{p:?}");
    }
    for p in [[0.5, 0.5], [-2.1, 0.0], [1.0, 1.0]] {
        let e = escape(Set::Mandelbrot, p, 500).expect("escapes");
        assert!(e.de > 0.0 && e.nu.is_finite(), "{p:?}");
    }
    // c = 0 gives the unit disc as its Julia set.
    assert!(escape(Set::Julia([0.0, 0.0]), [0.5, 0.3], 500).is_none());
    let e = escape(Set::Julia([0.0, 0.0]), [1.5, 0.0], 500).expect("escapes");
    // The distance estimate is within a factor of a few of the true distance, 0.5.
    assert!(e.de > 0.1 && e.de < 1.5, "{e:?}");
}

#[test]
fn deeper_views_get_more_iterations() {
    assert!(iterations(MIN_SPAN) > iterations(START_SPAN));
    assert!(iterations(1e-12) <= 600);
}

#[test]
fn world_and_plane_coordinates_round_trip() {
    let d = dive(Set::Mandelbrot, [-0.743, 0.131], 1e-3);
    for w in [[0.0, 0.0], [5.0, -3.0], [-20.0, 17.5]] {
        let back = d.to_world(d.to_plane(w));
        assert!((back[0] - w[0]).abs() < 1e-3 && (back[1] - w[1]).abs() < 1e-3);
    }
    assert_eq!(d.to_plane([0.0, 0.0]), d.centre);
}

#[test]
fn targets_lie_on_the_boundary() {
    let mut rng = Rng::new(7);
    for set in [
        Set::Mandelbrot,
        Set::Julia([-0.8, 0.156]),
        Set::Julia([0.285, 0.01]),
    ] {
        for _ in 0..3 {
            let t = find_target(&mut rng, set);
            // Close to the boundary: the target escapes slowly, or not at all, while
            // points a deep view's width away include ones that escape.
            let slow = escape(set, t, 600).is_none_or(|e| e.nu > 40.0);
            assert!(slow, "{set:?} {t:?}");
            let near_escapes = (0..16).any(|k| {
                let a = k as f64 / 16.0 * std::f64::consts::TAU;
                let p = [t[0] + MIN_SPAN * a.cos(), t[1] + MIN_SPAN * a.sin()];
                escape(set, p, iterations(MIN_SPAN)).is_some()
            });
            assert!(near_escapes, "{set:?} {t:?}");
        }
    }
}

#[test]
fn ridges_rise_on_the_boundary_and_bands_rise_with_their_frequency() {
    let quiet = [0.0; SPECTRUM_BINS];
    // Far outside the set: low plains. Inside: the mesa.
    let far = escape(Set::Mandelbrot, [2.5, 2.5], 200);
    assert!(height(far, 3.0, 0.0, &quiet) < 0.1);
    assert_eq!(height(None, 3.0, 0.0, &quiet), MESA_HEIGHT);
    // Just outside the cardioid's cusp at a fine scale: a ridge.
    let near = escape(Set::Mandelbrot, [0.2501, 0.0], 600);
    assert!(height(near, 0.05, 0.0, &quiet) > 0.5 * PEAK_HEIGHT);

    // Only the part of the spectrum matching a band's colour lifts it.
    let e = escape(Set::Mandelbrot, [0.5, 0.5], 200).expect("escapes");
    let t = band_of(e.nu, 0.0);
    let bin = (t * (SPECTRUM_BINS - 1) as f32).round() as usize;
    let mut matching = [0.0; SPECTRUM_BINS];
    matching[bin] = 1.0;
    let mut other = [0.0; SPECTRUM_BINS];
    other[(bin + SPECTRUM_BINS / 2) % SPECTRUM_BINS] = 1.0;
    let base = height(Some(e), 3.0, 0.0, &quiet);
    assert!(height(Some(e), 3.0, 0.0, &matching) > base + 0.1);
    assert!((height(Some(e), 3.0, 0.0, &other) - base).abs() < 1e-4);
}

#[test]
fn palettes_are_random_narrow_slices_of_the_colour_wheel() {
    let mut rng = Rng::new(9);
    let palettes: Vec<Palette> = (0..50).map(|_| Palette::random(&mut rng)).collect();
    for p in &palettes {
        assert!(p.range.abs() <= MAX_PALETTE_RANGE);
        // Dark at the bass end, bright at the treble end.
        let lum = |c: [f32; 3]| c[0] + c[1] + c[2];
        assert!(lum(p.colour(0.0)) < lum(p.colour(1.0)));
        assert!(p.colour(0.5).iter().all(|v| (0.0..=1.0).contains(v)));
    }
    let hues: Vec<f32> = palettes.iter().map(|p| p.hue).collect();
    assert!(hues.iter().any(|h| *h < 0.3) && hues.iter().any(|h| *h > 0.7));
}

#[test]
fn dives_zoom_in_then_dissolve_into_a_new_one() {
    let mut sim = MandelSim::new(3);
    let start = sim.dives().0.span;
    let first = sim.dives().0.id;
    let first_palette = sim.dives().0.palette;
    let mut saw_dissolve = false;
    for _ in 0..((MAX_DIVE + DISSOLVE + 1.0) / DT) as usize {
        sim.step(DT, &loud());
        let (current, next) = sim.dives();
        if current.id == first {
            assert!(current.span <= start);
        }
        if let Some((n, t)) = next {
            saw_dissolve = true;
            assert!((0.0..=1.0).contains(&t));
            assert_ne!(n.id, current.id);
        }
    }
    assert!(saw_dissolve);
    assert_ne!(sim.dives().0.id, first);
    assert_ne!(sim.dives().0.palette, first_palette);
    assert!(sim.dives().0.span >= MIN_SPAN * 0.5);
}

#[test]
fn lightning_arcs_between_peaks() {
    let mut sim = MandelSim::new(11);
    let mut saw_bolt = false;
    for _ in 0..(20.0 / DT) as usize {
        sim.step(DT, &loud());
        assert!(sim.segments().len() <= MAX_SEGMENTS);
        assert!(sim.lights().len() <= 3);
        for p in sim.peaks() {
            assert!(p[1] >= 0.5 * PEAK_HEIGHT);
        }
        if !sim.bolts().is_empty() {
            saw_bolt = true;
            for s in sim.segments() {
                assert!(s.a.iter().chain(&s.b).all(|v| v.is_finite()));
                assert!(s.intensity >= 0.0);
            }
        }
    }
    assert!(saw_bolt && sim.strikes() > 3, "strikes: {}", sim.strikes());
}

#[test]
fn silence_still_shows_a_slow_dive() {
    let mut sim = MandelSim::new(5);
    let start = sim.dives().0.span;
    for _ in 0..(5.0 / DT) as usize {
        sim.step(DT, &Drive::default());
    }
    let span = sim.dives().0.span;
    assert!(span < start && span > start * 0.25);
}
