use screendream::render::neuron_sim::{
    BANDS, CHAIN, Drive, MAX_MICROBES, MAX_NODES, NeuronSim, PULSE_SPEED, Phase,
};

const DT: f32 = 1.0 / 60.0;

fn quiet() -> Drive {
    Drive::default()
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Two nodes a few units apart, well ahead of the camera.
fn pair(gap: f32) -> (NeuronSim, usize, usize) {
    let mut sim = NeuronSim::empty(7);
    let a = sim.add_node([-gap / 2.0, 0.0, -10.0]);
    let b = sim.add_node([gap / 2.0, 0.0, -10.0]);
    (sim, a, b)
}

#[test]
fn a_bud_grows_opens_and_connects() {
    let (mut sim, a, b) = pair(3.0);
    let t = sim.grow(a, b).unwrap();
    let mut most_open_before_contact = 0.0f32;
    for _ in 0..60 * 15 {
        sim.step(DT, quiet());
        if sim.tendrils()[t].phase == Phase::Connected {
            break;
        }
        most_open_before_contact = most_open_before_contact.max(sim.tendrils()[t].open);
    }
    let tendril = &sim.tendrils()[t];
    assert_eq!(tendril.phase, Phase::Connected);
    assert!(
        most_open_before_contact > 0.7,
        "flower only opened to {most_open_before_contact}"
    );
    // The rope runs from one node to the other.
    assert!(distance(tendril.points[0], sim.nodes()[a].pos) < 1e-3);
    assert!(distance(tendril.points[CHAIN - 1], sim.nodes()[b].pos) < 1e-3);
}

#[test]
fn pulses_cascade_through_the_network() {
    let mut sim = NeuronSim::empty(3);
    let n: Vec<usize> = (0..4)
        .map(|i| sim.add_node([i as f32 * 2.5 - 4.0, 0.0, -10.0]))
        .collect();
    for w in n.windows(2) {
        sim.connect(w[0], w[1]);
    }
    sim.fire(n[0], 0, 1.0);
    assert_eq!(sim.pulses().len(), 1);
    let mut reached_end = false;
    for _ in 0..60 * 6 {
        sim.step(DT, quiet());
        reached_end |= sim.nodes()[n[3]].glow > 0.3;
    }
    assert!(reached_end, "the impulse never reached the far node");
    // Refractory periods stop it echoing forever.
    assert!(
        sim.pulses().is_empty(),
        "{} pulses still bouncing",
        sim.pulses().len()
    );
}

#[test]
fn treble_pulses_outrun_bass_pulses() {
    assert!(PULSE_SPEED.windows(2).all(|w| w[0] < w[1]));
    let travel = |band: usize| {
        let (mut sim, a, b) = pair(4.0);
        sim.connect(a, b);
        sim.fire(a, band, 1.0);
        for _ in 0..30 {
            sim.step(DT, quiet());
        }
        sim.pulses().first().map_or(1.0, |p| p.s)
    };
    assert!(travel(BANDS - 1) > travel(0) * 2.0);
}

#[test]
fn a_severed_tendril_recoils_retracts_and_the_nodes_spring_apart() {
    let (mut sim, a, b) = pair(4.0);
    let t = sim.connect(a, b).unwrap();
    // Let the spring take up its tension first.
    for _ in 0..60 * 3 {
        sim.step(DT, quiet());
    }
    let before = distance(sim.nodes()[a].pos, sim.nodes()[b].pos);
    sim.sever(t);
    assert_eq!(sim.tendrils().len(), 2);
    assert!(sim.tendrils().iter().all(|t| t.phase == Phase::Retracting));
    assert!(!sim.debris().is_empty(), "a snap should fling debris");
    for _ in 0..30 {
        sim.step(DT, quiet());
    }
    let after = distance(sim.nodes()[a].pos, sim.nodes()[b].pos);
    assert!(after > before + 0.1, "before {before}, after {after}");
    for _ in 0..60 * 5 {
        sim.step(DT, quiet());
    }
    assert!(
        sim.tendrils().is_empty(),
        "the halves should have withdrawn"
    );
}

#[test]
fn band_onsets_fire_pulses_of_that_band() {
    let mut sim = NeuronSim::new(5);
    let mut seen = [false; BANDS];
    for frame in 0..60 * 8 {
        let mut bands = [0.1; BANDS];
        // Each band thumps on its own rhythm.
        for (b, level) in bands.iter_mut().enumerate() {
            if frame % [40, 30, 20, 14][b] < 2 {
                *level = 0.9;
            }
        }
        sim.step(DT, Drive { bands, energy: 0.6 });
        for p in sim.pulses() {
            seen[p.band] = true;
        }
    }
    assert_eq!(seen, [true; BANDS]);
}

#[test]
fn the_culture_stays_alive_and_ahead_of_the_camera() {
    let mut sim = NeuronSim::new(9);
    let mut grew = 0;
    let mut severed = 0;
    let mut last: Vec<Phase> = Vec::new();
    for frame in 0..60 * 60 {
        let beat = if frame % 30 < 2 { 0.9 } else { 0.2 };
        sim.step(
            DT,
            Drive {
                bands: [beat, 0.3, 0.3, 0.2],
                energy: 0.6,
            },
        );
        if frame % 15 == 0 {
            let phases: Vec<Phase> = sim.tendrils().iter().map(|t| t.phase).collect();
            grew += phases
                .iter()
                .filter(|p| **p == Phase::Growing)
                .count()
                .min(1);
            severed += phases
                .iter()
                .filter(|p| **p == Phase::Retracting)
                .count()
                .min(1);
            last = phases;
        }
    }
    assert_eq!(sim.nodes().len(), MAX_NODES);
    assert_eq!(sim.microbes().len(), MAX_MICROBES);
    let connected = last.iter().filter(|p| **p == Phase::Connected).count();
    assert!(connected > 20, "only {connected} connections left");
    assert!(grew > 20 && severed > 5, "grew {grew}, severed {severed}");
    let cam = sim.camera();
    for n in sim.nodes() {
        let rel = [
            n.pos[0] - cam.pos[0],
            n.pos[1] - cam.pos[1],
            n.pos[2] - cam.pos[2],
        ];
        let ahead = rel[0] * cam.forward[0] + rel[1] * cam.forward[1] + rel[2] * cam.forward[2];
        assert!(ahead > -2.5, "node left behind at {ahead}");
    }
    for t in sim.tendrils() {
        assert!(t.points.iter().flatten().all(|v| v.is_finite()));
    }
    assert!(sim.focus() >= 2.0 && sim.focus() < 30.0);
}

#[test]
fn bacteria_swim_and_organelles_drift() {
    let mut sim = NeuronSim::new(4);
    let start: Vec<_> = sim.microbes().iter().map(|m| m.pos).collect();
    for _ in 0..60 {
        sim.step(DT, quiet());
    }
    let moved = sim
        .microbes()
        .iter()
        .zip(&start)
        .filter(|(m, s)| m.kind.swims() && distance(m.pos, **s) > 0.05)
        .count();
    assert!(moved > 5, "only {moved} swimmers moved");
}
