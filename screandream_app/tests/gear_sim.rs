use screendream::render::gear_sim::{
    BarStyle, Drive, FOCAL, GearKind, GearSim, LAYER_Z, LAYERS, LinkageKind, MAX_SPARKS, Motion,
};
use std::f32::consts::TAU;

const DT: f32 = 1.0 / 60.0;

fn bass_at(frame: usize, period: usize) -> f32 {
    if frame % period < 3 { 0.9 } else { 0.1 }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// For every engaged meshing pair: centres a pitch-radius sum apart, opposite spin at the
/// tooth ratio, and a tooth of one facing a gap of the other at the contact point.
fn assert_meshes(sim: &GearSim) {
    let mut pairs = 0;
    for (_, g) in sim.gears() {
        if !g.engaged() || g.on_axle {
            continue;
        }
        let Some(p) = g.driver.and_then(|d| sim.gear(d)) else {
            continue;
        };
        pairs += 1;
        let d = dist(g.home, p.home);
        assert!(
            (d - (g.radius + p.radius)).abs() < 1e-3,
            "centres {d} apart"
        );
        let ratio = g.ratio / p.ratio;
        let expected = -(p.teeth as f32) / g.teeth as f32;
        assert!(
            (ratio - expected).abs() < 1e-3,
            "ratio {ratio} vs {expected}"
        );
        let heading = (g.home[1] - p.home[1]).atan2(g.home[0] - p.home[0]);
        let phase = |angle_at: f32, angle: f32, teeth: u32| {
            ((angle_at - angle) * teeth as f32 / TAU).rem_euclid(1.0)
        };
        let sum = phase(heading, p.angle, p.teeth)
            + phase(heading + std::f32::consts::PI, g.angle, g.teeth);
        let off = (sum - 0.5).rem_euclid(1.0);
        assert!(
            off.min(1.0 - off) < 0.01,
            "teeth clash: phases sum to {sum}"
        );
    }
    assert!(pairs > 30, "only {pairs} meshing pairs");
}

#[test]
fn every_meshing_pair_fits_tooth_into_gap() {
    let mut sim = GearSim::new(1);
    assert_meshes(&sim);
    // Still true after turning, ticking and reversing.
    for frame in 0..60 * 10 {
        sim.step(
            DT,
            Drive {
                bass: bass_at(frame, 25),
                energy: 0.7,
                treble: 0.3,
            },
        );
    }
    assert_meshes(&sim);
}

#[test]
fn gears_in_a_layer_never_overlap() {
    for seed in 1..=8 {
        check_no_overlap(&GearSim::new(seed));
    }
}

fn check_no_overlap(sim: &GearSim) {
    let gears: Vec<_> = sim.gears().map(|(i, g)| (i, *g)).collect();
    for (i, a) in &gears {
        for (j, b) in &gears {
            if i >= j || a.layer != b.layer {
                continue;
            }
            let meshing =
                (a.driver == Some(*j) && !a.on_axle) || (b.driver == Some(*i) && !b.on_axle);
            if meshing {
                continue;
            }
            let d = dist(a.home, b.home);
            assert!(
                d >= a.outer_radius() + b.outer_radius(),
                "gears {i} and {j} overlap"
            );
        }
    }
}

#[test]
fn speeds_stay_bounded() {
    let sim = GearSim::new(3);
    for (_, g) in sim.gears() {
        if g.kind != GearKind::Balance {
            assert!(
                g.ratio.abs() <= 6.0 + 1e-3 && g.ratio.abs() >= 0.06 - 1e-3,
                "{}",
                g.ratio
            );
        }
    }
}

#[test]
fn the_mechanism_fills_the_screen() {
    for seed in 1..=8 {
        check_coverage(&GearSim::new(seed));
    }
}

fn check_coverage(sim: &GearSim) {
    let aspect = 16.0 / 9.0;
    let (mut covered, mut total) = (0, 0);
    for yi in 0..40 {
        for xi in 0..70 {
            let s = [
                (xi as f32 / 69.0 * 2.0 - 1.0) * aspect,
                yi as f32 / 39.0 * 2.0 - 1.0,
            ];
            total += 1;
            let hit = sim.gears().any(|(_, g)| {
                let z = LAYER_Z[g.layer];
                let world = [s[0] * z / FOCAL, s[1] * z / FOCAL];
                dist(world, g.home) < g.outer_radius()
            });
            covered += hit as usize;
        }
    }
    let share = covered as f32 / total as f32;
    assert!(share > 0.85, "only {:.0}% covered", share * 100.0);
    for layer in 0..LAYERS {
        assert!(sim.gears().filter(|(_, g)| g.layer == layer).count() > 15);
    }
}

#[test]
fn beats_tick_the_train_and_sometimes_reverse_it() {
    let mut sim = GearSim::new(5);
    let start = sim.drive_angle();
    let mut directions = Vec::new();
    for frame in 0..60 * 30 {
        sim.step(
            DT,
            Drive {
                bass: bass_at(frame, 30),
                energy: 0.5,
                treble: 0.0,
            },
        );
        directions.push(sim.direction());
    }
    assert!(sim.reversals() >= 2, "reversed {} times", sim.reversals());
    assert!(directions.contains(&1.0) && directions.contains(&-1.0));
    assert!(sim.drive_angle() != start);

    // Without beats it creeps steadily one way.
    let mut calm = GearSim::new(5);
    let before = calm.drive_angle();
    for _ in 0..60 * 5 {
        calm.step(
            DT,
            Drive {
                bass: 0.0,
                energy: 0.3,
                treble: 0.0,
            },
        );
    }
    assert_eq!(calm.reversals(), 0);
    assert!(calm.drive_angle() > before + 0.5);
}

#[test]
fn gears_are_swapped_out_and_replaced() {
    let mut sim = GearSim::new(6);
    let start_count = sim.count();
    let (mut saw_leaving, mut saw_arriving, mut saw_sparks) = (false, false, false);
    for frame in 0..60 * 40 {
        sim.step(
            DT,
            Drive {
                bass: bass_at(frame, 28),
                energy: 0.8,
                treble: 0.4,
            },
        );
        for (_, g) in sim.gears() {
            match g.motion {
                Motion::Leaving { t } => {
                    saw_leaving = true;
                    // Solid as it lifts off, faded away by the end of its path.
                    if t < 0.6 {
                        assert_eq!(g.fade, 1.0);
                    }
                    if t > 0.97 {
                        assert!(g.fade < 0.05, "still {} opaque at t = {t}", g.fade);
                    }
                    assert_eq!(g.layer, LAYERS - 1, "only the top layer swaps");
                    assert!(
                        g.pos[2] <= LAYER_Z[g.layer] + 1e-3,
                        "leaving gears never sink through the ones below"
                    );
                }
                Motion::Arriving { t } => {
                    saw_arriving = true;
                    // Fades in up high before it starts to fall.
                    if t < 0.03 {
                        assert!(g.fade < 0.1, "arrived already {} opaque", g.fade);
                    }
                    if t < 0.2 {
                        assert!(g.pos[2] < LAYER_Z[0] - 1.5, "falling before it faded in");
                    }
                    assert_eq!(g.layer, LAYERS - 1, "only the top layer swaps");
                    assert!(
                        g.pos[2] <= LAYER_Z[g.layer] + 1e-3,
                        "arrivals come from above"
                    );
                }
                Motion::Engaged => {}
            }
        }
        saw_sparks |= !sim.sparks().is_empty();
        assert!(sim.sparks().len() <= MAX_SPARKS);
    }
    assert!(sim.swaps() >= 40, "only {} swaps", sim.swaps());
    assert!(saw_leaving && saw_arriving && saw_sparks);
    let end = sim.count();
    assert!(
        end as f32 > start_count as f32 * 0.9,
        "{start_count} gears became {end}"
    );
    // Swaps and top-ups never break the meshing.
    assert_meshes(&sim);
}

fn length(a: [f32; 2], b: [f32; 2]) -> f32 {
    dist(a, b)
}

#[test]
fn linkages_hold_together_as_they_move() {
    for seed in 1..=6 {
        let mut sim = GearSim::new(seed);
        assert!(
            sim.linkages().len() >= 2,
            "seed {seed}: {} linkages",
            sim.linkages().len()
        );
        let mut kinds = Vec::new();
        for frame in 0..60 * 8 {
            sim.step(
                DT,
                Drive {
                    bass: bass_at(frame, 25),
                    energy: 0.8,
                    treble: 0.3,
                },
            );
            for l in sim.linkages() {
                let gear = sim
                    .gear(l.driver)
                    .expect("a linkage's gear is never swapped out");
                assert!(gear.engaged());
                // The crank pin rides on the gear's face.
                assert!((length(l.crank_pin, gear.home) - l.crank).abs() < 1e-3);
                match l.kind {
                    LinkageKind::WalkingBeam => {
                        let [near, far] = l.beam_ends;
                        assert!((length(near, l.pivot) - l.beam_half).abs() < 1e-3);
                        assert!((length(far, l.pivot) - l.beam_half).abs() < 1e-3);
                        assert!((length(l.crank_pin, near) - l.drive_rod).abs() < 1e-3);
                        assert!((length(far, l.piston) - l.rod).abs() < 1e-3);
                    }
                    LinkageKind::SliderCrank => {
                        assert!((length(l.crank_pin, l.piston) - l.rod).abs() < 1e-3);
                    }
                    LinkageKind::ScotchYoke => {}
                }
                kinds.push(l.kind);
            }
        }
        assert!(sim.bars().iter().any(|b| b.style == BarStyle::Cylinder));
        if seed == 1 {
            assert!(kinds.contains(&LinkageKind::WalkingBeam));
        }
    }
}

#[test]
fn steam_chuffs_and_sparks_fly() {
    let mut sim = GearSim::new(8);
    let (mut most_sparks, mut most_puffs, mut embers) = (0, 0, false);
    for frame in 0..60 * 20 {
        sim.step(
            DT,
            Drive {
                bass: bass_at(frame, 28),
                energy: 0.8,
                treble: 0.6,
            },
        );
        most_sparks = most_sparks.max(sim.sparks().len());
        most_puffs = most_puffs.max(sim.puffs().len());
        embers |= sim.sparks().iter().any(|s| s.ember);
    }
    assert!(most_sparks > 300, "only {most_sparks} sparks at once");
    assert!(most_puffs > 150, "only {most_puffs} puffs of smoke");
    let mut tints: Vec<[u32; 3]> = sim
        .puffs()
        .iter()
        .map(|p| p.tint.map(|c| (c * 100.0) as u32))
        .collect();
    tints.sort();
    tints.dedup();
    assert!(
        tints.len() >= 3,
        "smoke comes in only {} colours",
        tints.len()
    );
    assert!(embers);
    assert!(sim.motes().len() >= 300);
}

#[test]
fn reversals_are_frequent_and_change_the_speed() {
    let mut sim = GearSim::new(12);
    let mut speeds = Vec::new();
    for frame in 0..60 * 30 {
        sim.step(
            DT,
            Drive {
                bass: bass_at(frame, 30),
                energy: 0.5,
                treble: 0.0,
            },
        );
        let s = (sim.speed_scale() * 100.0) as u32;
        if speeds.last() != Some(&s) {
            speeds.push(s);
        }
    }
    // A beat every half second for 30 s: a reversal every bar or two.
    assert!(sim.reversals() >= 8, "only {} reversals", sim.reversals());
    assert!(speeds.len() >= 5, "speeds {speeds:?}");
}

#[test]
fn spotlights_strafe_the_mechanism() {
    let mut sim = GearSim::new(13);
    let spots: Vec<usize> = (0..sim.lights().len())
        .filter(|&i| sim.lights()[i].cone.is_some())
        .collect();
    assert!(spots.len() >= 2);
    let aim = |sim: &GearSim, i: usize| {
        let l = sim.lights()[i];
        // Where the beam lands on the mechanism.
        let t = (LAYER_Z[1] - l.pos[2]) / l.dir[2];
        [l.pos[0] + l.dir[0] * t, l.pos[1] + l.dir[1] * t]
    };
    let start: Vec<_> = spots.iter().map(|&i| aim(&sim, i)).collect();
    for _ in 0..60 * 3 {
        sim.step(
            DT,
            Drive {
                bass: 0.2,
                energy: 0.6,
                treble: 0.0,
            },
        );
    }
    for (k, &i) in spots.iter().enumerate() {
        let now = aim(&sim, i);
        assert!(dist(now, start[k]) > 0.3, "spotlight {i} barely moved");
        assert!(
            now[0].abs() < 2.0 && now[1].abs() < 1.2,
            "spotlight {i} aimed off the works"
        );
    }
}
