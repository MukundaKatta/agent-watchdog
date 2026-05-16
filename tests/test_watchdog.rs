use agent_watchdog::{Trip, Watchdog};
use std::thread;
use std::time::Duration;

#[test]
fn unlimited_watchdog_never_trips() {
    let w = Watchdog::new();
    for _ in 0..1000 {
        w.check().unwrap();
        w.record_step();
        w.record_cost_usd(1.0);
        w.record_tokens(10_000);
    }
}

#[test]
fn max_steps_trips() {
    let w = Watchdog::new().with_max_steps(3);
    assert!(w.check().is_ok());
    w.record_step();
    assert!(w.check().is_ok());
    w.record_step();
    assert!(w.check().is_ok());
    w.record_step();
    assert_eq!(w.check(), Err(Trip::MaxSteps));
}

#[test]
fn max_tokens_trips() {
    let w = Watchdog::new().with_max_tokens(1000);
    w.record_tokens(500);
    assert!(w.check().is_ok());
    w.record_tokens(500);
    assert_eq!(w.check(), Err(Trip::MaxTokens));
}

#[test]
fn max_cost_trips() {
    let w = Watchdog::new().with_max_cost_usd(0.01);
    w.record_cost_usd(0.005);
    assert!(w.check().is_ok());
    w.record_cost_usd(0.005);
    assert_eq!(w.check(), Err(Trip::MaxCostUsd));
}

#[test]
fn timeout_trips_after_deadline() {
    let w = Watchdog::new().with_timeout(Duration::from_millis(50));
    assert!(w.check().is_ok());
    thread::sleep(Duration::from_millis(80));
    assert_eq!(w.check(), Err(Trip::Timeout));
}

#[test]
fn timeout_does_not_trip_before_deadline() {
    let w = Watchdog::new().with_timeout(Duration::from_secs(60));
    assert!(w.check().is_ok());
    thread::sleep(Duration::from_millis(20));
    assert!(w.check().is_ok());
}

#[test]
fn time_remaining_decreases() {
    let w = Watchdog::new().with_timeout(Duration::from_millis(200));
    let r1 = w.time_remaining().unwrap();
    thread::sleep(Duration::from_millis(50));
    let r2 = w.time_remaining().unwrap();
    assert!(r2 < r1);
}

#[test]
fn counters_track_correctly() {
    let w = Watchdog::new();
    w.record_step();
    w.record_step();
    w.record_tokens(100);
    w.record_tokens(200);
    w.record_cost_usd(0.5);
    w.record_cost_usd(0.25);
    let (steps, tokens, cost) = w.counters();
    assert_eq!(steps, 2);
    assert_eq!(tokens, 300);
    assert!((cost - 0.75).abs() < 1e-9);
}

#[test]
fn clones_share_counters() {
    let w = Watchdog::new().with_max_steps(5);
    let w2 = w.clone();
    w.record_step();
    w2.record_step();
    let (steps, _, _) = w2.counters();
    assert_eq!(steps, 2);
}

#[test]
fn priority_order_timeout_first() {
    // If timeout AND max_steps could both trip, timeout wins (it's the
    // most "external" cause).
    let w = Watchdog::new()
        .with_timeout(Duration::from_millis(50))
        .with_max_steps(1);
    thread::sleep(Duration::from_millis(80));
    w.record_step(); // would also trip max_steps now
    assert_eq!(w.check(), Err(Trip::Timeout));
}

#[test]
fn no_timeout_returns_none_for_remaining() {
    let w = Watchdog::new();
    assert!(w.time_remaining().is_none());
}

#[test]
fn loop_pattern_runs_to_completion() {
    // Simulate a 3-step agent loop with all caps generous enough to allow it.
    let w = Watchdog::new()
        .with_timeout(Duration::from_secs(5))
        .with_max_steps(10)
        .with_max_tokens(10_000)
        .with_max_cost_usd(1.0);

    let mut completed = 0;
    for _ in 0..3 {
        if w.check().is_err() {
            break;
        }
        w.record_step();
        w.record_tokens(500);
        w.record_cost_usd(0.01);
        completed += 1;
    }
    assert_eq!(completed, 3);
}
