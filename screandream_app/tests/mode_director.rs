use screendream::render::mode_director::{DirectorConfig, ModeDirector};
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_millis(16);

fn run(director: &mut ModeDirector, from: Instant, for_: Duration, audio: bool) -> Vec<usize> {
    let mut switches = Vec::new();
    let mut t = Duration::ZERO;
    while t < for_ {
        if let Some(mode) = director.tick(from + t, audio) {
            switches.push(mode);
        }
        t += FRAME;
    }
    switches
}

#[test]
fn starts_on_the_requested_mode() {
    let director = ModeDirector::new(4, 2, Instant::now(), 7);
    assert_eq!(director.current(), 2);
}

#[test]
fn switches_after_the_maximum_duration() {
    let start = Instant::now();
    let mut director = ModeDirector::new(3, 0, start, 7);
    assert!(run(&mut director, start, Duration::from_secs(299), true).is_empty());
    let switched = director.tick(start + Duration::from_secs(300), true);
    assert!(matches!(switched, Some(mode) if mode != 0));
}

#[test]
fn a_gap_between_songs_switches_mode() {
    let start = Instant::now();
    let mut director = ModeDirector::new(3, 0, start, 7);
    assert!(run(&mut director, start, Duration::from_secs(60), true).is_empty());
    let gap_start = start + Duration::from_secs(60);
    assert!(run(&mut director, gap_start, Duration::from_secs(2), false).is_empty());
    let resumed = director.tick(gap_start + Duration::from_secs(2), true);
    assert!(matches!(resumed, Some(mode) if mode != 0));
}

#[test]
fn short_pauses_and_early_gaps_do_not_switch() {
    let start = Instant::now();
    let mut director = ModeDirector::new(3, 0, start, 7);
    // A gap before the minimum play time is ignored.
    run(&mut director, start, Duration::from_secs(5), false);
    assert_eq!(director.tick(start + Duration::from_secs(5), true), None);
    // A brief dip in the music is not a song change.
    let later = start + Duration::from_secs(60);
    run(&mut director, later, Duration::from_millis(500), false);
    assert_eq!(
        director.tick(later + Duration::from_millis(500), true),
        None
    );
}

#[test]
fn next_never_repeats_the_current_mode() {
    let start = Instant::now();
    let mut director = ModeDirector::with_config(DirectorConfig::default(), 3, 0, start, 99);
    let mut previous = director.current();
    for _ in 0..50 {
        let mode = director.next(start);
        assert_ne!(mode, previous);
        assert!(mode < 3);
        previous = mode;
    }
}

#[test]
fn a_single_mode_stays_put() {
    let start = Instant::now();
    let mut director = ModeDirector::new(1, 0, start, 3);
    assert_eq!(director.next(start), 0);
}
