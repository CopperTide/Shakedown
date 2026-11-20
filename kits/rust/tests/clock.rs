//! The fixed-step clock: the way to meet the clock contract (the state after `advance` does not depend
//! on how the advances were split).
use shakedown_part::helpers::clock::FixedStep;

#[test]
fn whole_steps_run_and_the_remainder_waits() {
    let mut clock = FixedStep::new(100);
    let mut ran = vec![];
    assert_eq!(clock.advance_to(250, |ms| ran.push(ms)), 2);
    assert_eq!((clock.done_ms(), ran.as_slice()), (200, &[100, 100][..]));
    assert_eq!(clock.advance_to(299, |_| panic!("no whole step")), 0);
    assert_eq!(clock.advance_to(300, |_| {}), 1);
    assert_eq!(clock.advance_to(100, |_| panic!("behind")), 0);
}

#[test]
fn how_the_advances_are_split_does_not_matter() {
    let run = |to: &[u64]| {
        let mut clock = FixedStep::new(30);
        let mut n = 0;
        for t in to {
            clock.advance_to(*t, |_| n += 1);
        }
        (n, clock.done_ms())
    };
    assert_eq!(run(&[1000]), run(&[7, 61, 62, 500, 999, 1000]));
}

#[test]
fn a_clock_may_start_at_the_last_whole_step_before_a_time() {
    assert_eq!(FixedStep::starting_at(500, 1_250).done_ms(), 1_000);
    let mut clock = FixedStep::new(500);
    clock.set_done_ms(3_000);
    assert_eq!(clock.advance_to(3_600, |_| {}), 1);
}

#[test]
#[should_panic(expected = "step_ms must be positive")]
fn a_step_must_be_positive() {
    FixedStep::new(0);
}
