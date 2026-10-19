use screendream::render::automata_sim::{
    AutomataSim, Automaton, Drive, GRID, MAX_AUTOMATA, MAX_WARPS, RULES, Rng, cross_seed,
    rule_index,
};

const DT: f32 = 1.0 / 60.0;
const ASPECT: f32 = 1.6;

fn rule(name: &str) -> usize {
    rule_index(name).unwrap_or_else(|| panic!("no rule {name}"))
}

/// Bass that thumps every `period` frames.
fn bass_at(frame: usize, period: usize) -> f32 {
    if frame % period < 3 { 0.9 } else { 0.1 }
}

#[test]
fn catalogue_covers_every_wolfram_class() {
    for class in 1..=4 {
        assert!(
            RULES.iter().any(|r| r.class == class),
            "no rule of class {class}"
        );
    }
}

#[test]
fn life_blinker_oscillates() {
    let mut a = Automaton::blank(rule("Life"), 16);
    for x in 6..9 {
        a.set(x, 7, 1);
    }
    a.advance();
    let vertical: Vec<_> = (6..9).map(|y| a.state(7, y)).collect();
    assert_eq!(vertical, [1, 1, 1]);
    assert_eq!(a.state(6, 7), 0);
    a.advance();
    assert_eq!((6..9).map(|x| a.state(x, 7)).collect::<Vec<_>>(), [1, 1, 1]);
}

#[test]
fn generations_rules_leave_dying_cells() {
    let mut a = Automaton::blank(rule("Brian's Brain"), 16);
    a.set(5, 5, 1);
    a.advance();
    assert_eq!(a.state(5, 5), 2, "a firing cell becomes refractory");
    a.advance();
    assert_eq!(a.state(5, 5), 0);
}

#[test]
fn rule_90_draws_a_sierpinski_triangle() {
    let mut a = Automaton::blank(rule("Rule 90"), 33);
    a.set(16, 0, 1);
    for _ in 0..4 {
        a.advance();
    }
    let row = |y: usize| (12..=20).map(|x| a.state(x, y)).collect::<Vec<_>>();
    assert_eq!(row(1), [0, 0, 0, 1, 0, 1, 0, 0, 0]);
    assert_eq!(row(2), [0, 0, 1, 0, 0, 0, 1, 0, 0]);
    assert_eq!(row(3), [0, 1, 0, 1, 0, 1, 0, 1, 0]);
}

#[test]
fn elementary_diagram_scrolls_once_full() {
    let mut a = Automaton::blank(rule("Rule 30"), 8);
    a.set(4, 0, 1);
    for _ in 0..20 {
        a.advance();
    }
    // Still growing at the bottom row rather than stopping.
    assert!((0..8).any(|x| a.state(x, 7) == 1));
}

#[test]
fn cells_stay_inside_the_growth_front() {
    let mut a = Automaton::blank(rule("Seeds"), 64);
    let mut rng = Rng::new(3);
    a.radius = 0.0;
    a.grow(6.0, &mut rng);
    for x in 30..34 {
        a.set(x, 32, 1);
    }
    for _ in 0..30 {
        a.advance();
    }
    let c = 31.5;
    for y in 0..64 {
        for x in 0..64 {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            if d > 6.0 {
                assert_eq!(a.state(x, y), 0, "cell ({x}, {y}) outside the front");
            }
        }
    }
}

#[test]
fn crossing_planes_seed_each_other_with_their_own_colours() {
    let life = rule("Life");
    let mut source = Automaton::blank(life, 32);
    for y in 0..32 {
        for x in 0..32 {
            source.set(x, y, 1);
        }
    }
    // The target stands at right angles, crossing the source along the y axis.
    let mut target = Automaton::blank(life, 32);
    target.axis_u = [0.0, 0.0, 1.0];
    let mut rng = Rng::new(9);
    let seeded = cross_seed(&source, 4, &mut target, 7, 1.0, &mut rng);
    assert!(seeded > 20, "only {seeded} cells seeded");
    // Along the seam (target x ≈ the middle column), seeded cells carry owner 4 + 1.
    let seam: Vec<_> = (0..32).filter(|&y| target.state(16, y) == 1).collect();
    assert!(seam.len() > 20, "seam {seam:?}");
    assert!(seam.iter().all(|&y| target.owner(16, y) == 5));
    // Away from the seam nothing changed.
    assert!((0..32).all(|y| target.state(2, y) == 0));
}

#[test]
fn parallel_planes_do_not_interact() {
    let life = rule("Life");
    let mut source = Automaton::blank(life, 16);
    source.set(8, 8, 1);
    let mut target = Automaton::blank(life, 16);
    target.center = [0.0, 0.0, 0.5];
    let seeded = cross_seed(&source, 0, &mut target, 1, 1.0, &mut Rng::new(1));
    assert_eq!(seeded, 0);
}

fn generations_after(seconds: usize, beat_period: Option<usize>, energy: f32) -> u64 {
    let mut sim = AutomataSim::new(11);
    let start = sim.total_generations();
    for frame in 0..60 * seconds {
        let bass = beat_period.map_or(0.0, |p| bass_at(frame, p));
        sim.step(
            DT,
            ASPECT,
            Drive {
                bass,
                energy,
                treble: 0.3,
            },
        );
    }
    sim.total_generations() - start
}

#[test]
fn growth_follows_the_beat() {
    let silent = generations_after(6, None, 0.0);
    let slow = generations_after(6, Some(45), 0.5); // 80 bpm
    let fast = generations_after(6, Some(22), 0.5); // ~164 bpm
    assert!(slow > silent * 2, "silent {silent}, slow {slow}");
    assert!(fast as f32 > slow as f32 * 1.4, "slow {slow}, fast {fast}");
}

#[test]
fn tempo_is_tracked() {
    let mut sim = AutomataSim::new(2);
    for frame in 0..60 * 6 {
        sim.step(
            DT,
            ASPECT,
            Drive {
                bass: bass_at(frame, 30),
                energy: 0.5,
                treble: 0.0,
            },
        );
    }
    let bps = sim.beats_per_second();
    assert!((bps - 2.0).abs() < 0.3, "expected ~2 beats/s, got {bps}");
    assert!(sim.beats() >= 10);
}

#[test]
fn field_stays_full_and_ahead_of_the_camera() {
    let mut sim = AutomataSim::new(5);
    for frame in 0..60 * 40 {
        sim.step(
            DT,
            ASPECT,
            Drive {
                bass: bass_at(frame, 28),
                energy: 0.8,
                treble: 0.5,
            },
        );
    }
    assert_eq!(sim.automata().len(), MAX_AUTOMATA);
    let cam = sim.camera();
    for a in sim.automata() {
        let rel = [
            a.center[0] - cam.pos[0],
            a.center[1] - cam.pos[1],
            a.center[2] - cam.pos[2],
        ];
        let ahead = rel[0] * cam.forward[0] + rel[1] * cam.forward[1] + rel[2] * cam.forward[2];
        assert!(ahead > -a.half_size * 1.6, "automaton left behind: {ahead}");
        assert!(a.dim <= GRID);
    }
    // Something is alive out there.
    assert!(sim.automata().iter().filter(|a| a.activity > 0).count() > MAX_AUTOMATA / 2);
}

#[test]
fn screen_warps_traverse_and_beats_add_shockwaves() {
    let mut sim = AutomataSim::new(8);
    let mut most = 0;
    for frame in 0..60 * 20 {
        sim.step(
            DT,
            ASPECT,
            Drive {
                bass: bass_at(frame, 30),
                energy: 0.6,
                treble: 0.0,
            },
        );
        most = most.max(sim.warps().len());
        for w in sim.warps() {
            assert!(w.envelope() >= 0.0 && w.envelope() <= 1.0);
            assert!(w.pos[0].abs() < 6.0 && w.pos[1].abs() < 6.0);
        }
    }
    assert!(most > 3 && most <= MAX_WARPS, "{most} warps");
}

#[test]
fn texels_encode_live_cells() {
    let mut a = Automaton::blank(rule("Life"), 8);
    a.set(3, 2, 1);
    let mut texels = vec![0; 8 * 8 * 4];
    a.write_texels(&mut texels);
    let o = (2 * 8 + 3) * 4;
    assert_eq!(texels[o], 255);
    assert_eq!(texels[o + 3], 255, "a newborn cell is fresh");
    assert_eq!(texels[0], 0);
    assert!(a.take_dirty());
    assert!(!a.take_dirty());
}
