use screendream::render::bubble_sim::{
    BANDS, BubbleSim, MAX_BUBBLES, MAX_POPS, SEABED_Y, SURFACE_Y, target_radius,
};

const DT: f32 = 1.0 / 60.0;
const ASPECT: f32 = 1.6;

fn count_after(seconds: usize, level: f32, seed: u64) -> usize {
    let mut sim = BubbleSim::new(seed);
    let levels = [level; BANDS];
    for _ in 0..60 * seconds {
        sim.step(DT, ASPECT, &levels, 0.0, 0.0);
    }
    sim.bubbles().len()
}

#[test]
fn more_intense_music_releases_more_bubbles() {
    let calm = count_after(8, 0.15, 5);
    let intense = count_after(8, 0.9, 5);
    assert!(intense > calm * 2, "calm {calm}, intense {intense}");
}

#[test]
fn beats_set_the_water_sloshing_back_and_forth() {
    let mut sim = BubbleSim::new(1);
    let levels = [0.5; BANDS];
    let (mut most_left, mut most_right) = (0.0f32, 0.0f32);
    for frame in 0..60 * 8 {
        // A kick every half second, decaying between beats.
        let beat_phase = (frame % 30) as f32 / 30.0;
        let kick = if beat_phase < 0.05 {
            1.0
        } else {
            (1.0 - beat_phase).powi(4)
        };
        sim.step(DT, ASPECT, &levels, 0.3, kick);
        most_left = most_left.min(sim.current());
        most_right = most_right.max(sim.current());
    }
    assert!(
        most_left < -0.05 && most_right > 0.05,
        "current {most_left}..{most_right}"
    );

    // Without beats the swell dies away to the slow background drift.
    for _ in 0..60 * 20 {
        sim.step(DT, ASPECT, &levels, 0.3, 0.0);
    }
    assert!(
        sim.current().abs() < 0.04,
        "residual current {}",
        sim.current()
    );
}

#[test]
fn louder_bands_and_lower_frequencies_make_bigger_bubbles() {
    assert!(target_radius(0, 1.0, 1.0) > target_radius(0, 1.0, 0.0));
    for band in 1..BANDS {
        assert!(target_radius(band, 1.0, 0.5) < target_radius(band - 1, 1.0, 0.5));
    }
}

#[test]
fn bubbles_rise_from_the_seabed_and_pop_at_the_surface() {
    let mut sim = BubbleSim::new(3);
    let quiet = [0.0; BANDS];
    let mut highest = SEABED_Y;
    let mut popped = false;
    for _ in 0..60 * 40 {
        sim.step(DT, ASPECT, &quiet, 0.0, 0.0);
        for b in sim.bubbles() {
            highest = highest.max(b.pos[1]);
            assert!(b.pos[1] <= SURFACE_Y + b.radius, "bubble above the surface");
        }
        popped |= !sim.pops().is_empty();
    }
    assert!(highest > 0.5, "bubbles should rise; highest {highest}");
    assert!(popped, "bubbles should pop at the surface");
}

#[test]
fn loud_music_splits_bubbles_but_stays_within_limits() {
    let mut sim = BubbleSim::new(9);
    let loud = [1.0; BANDS];
    let mut max_band = 0;
    for frame in 0..60 * 30 {
        let kick = if frame % 30 == 0 { 1.0 } else { 0.0 };
        sim.step(DT, ASPECT, &loud, 1.0, kick);
        assert!(sim.bubbles().len() <= MAX_BUBBLES);
        assert!(sim.pops().len() <= MAX_POPS);
        for b in sim.bubbles() {
            assert!(b.band < BANDS);
            assert!(b.radius.is_finite() && b.radius >= 0.0);
            max_band = max_band.max(b.band);
        }
    }
    assert!(sim.bubbles().len() > 20, "count {}", sim.bubbles().len());
    assert_eq!(max_band, BANDS - 1, "splits should reach the smallest band");
}
