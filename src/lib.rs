//! Deadline + budget watchdog for LLM agent loops.
//!
//! Agent loops drift when nothing kills them. This crate is the small,
//! focused primitive that does: build a [`Watchdog`] with a deadline and
//! optional caps (max steps, max tokens, max cost), call
//! [`Watchdog::check`] at the top of each agent turn, and on
//! [`Trip`] reason stop the loop cleanly.
//!
//! Designed to compose with `cachebench` / `claude-cost` / `agenttrace`:
//! you compute cost per call there, hand the number to
//! [`Watchdog::record`], the watchdog accumulates and trips when it
//! exceeds your cap.
//!
//! # Quick start
//!
//! ```
//! use agent_watchdog::{Watchdog, Trip};
//! use std::time::Duration;
//!
//! let watchdog = Watchdog::new()
//!     .with_timeout(Duration::from_secs(30))
//!     .with_max_steps(10)
//!     .with_max_cost_usd(0.10);
//!
//! for step in 0..100 {
//!     match watchdog.check() {
//!         Ok(()) => { /* run one agent step */ }
//!         Err(Trip::Timeout) => break,
//!         Err(Trip::MaxSteps) => break,
//!         Err(Trip::MaxCostUsd) => break,
//!         Err(Trip::MaxTokens) => break,
//!     }
//!     watchdog.record_step();
//!     watchdog.record_cost_usd(0.012);
//!     watchdog.record_tokens(420);
//! }
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Reason a watchdog tripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Trip {
    /// Wall-clock deadline reached.
    #[error("watchdog: timeout")]
    Timeout,
    /// `max_steps` reached.
    #[error("watchdog: max steps")]
    MaxSteps,
    /// `max_tokens` total reached.
    #[error("watchdog: max tokens")]
    MaxTokens,
    /// `max_cost_usd` total reached.
    #[error("watchdog: max cost USD")]
    MaxCostUsd,
}

#[derive(Default)]
struct Counters {
    steps: usize,
    tokens: u64,
    cost_usd: f64,
}

struct Inner {
    deadline: Option<Instant>,
    max_steps: Option<usize>,
    max_tokens: Option<u64>,
    max_cost_usd: Option<f64>,
    counters: Mutex<Counters>,
}

/// Deadline + budget watchdog. Cheap to clone; clones share the same counters.
#[derive(Clone)]
pub struct Watchdog {
    inner: Arc<Inner>,
}

impl Watchdog {
    /// New watchdog with no caps. Call `.with_*` to add limits.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                deadline: None,
                max_steps: None,
                max_tokens: None,
                max_cost_usd: None,
                counters: Mutex::new(Counters::default()),
            }),
        }
    }

    /// Add a wall-clock timeout, starting now.
    pub fn with_timeout(self, dur: Duration) -> Self {
        // If the Arc is shared, fall back to rebuilding a fresh Inner.
        // Builders are typically called before clones happen, so this is a
        // defensive fallback.
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(rebuild_inner);
        Self {
            inner: Arc::new(Inner {
                deadline: Some(Instant::now() + dur),
                ..inner
            }),
        }
    }

    /// Cap the number of agent steps.
    pub fn with_max_steps(self, n: usize) -> Self {
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(rebuild_inner);
        Self {
            inner: Arc::new(Inner {
                max_steps: Some(n),
                ..inner
            }),
        }
    }

    /// Cap cumulative tokens across all recorded calls.
    pub fn with_max_tokens(self, n: u64) -> Self {
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(rebuild_inner);
        Self {
            inner: Arc::new(Inner {
                max_tokens: Some(n),
                ..inner
            }),
        }
    }

    /// Cap cumulative cost in USD across all recorded calls.
    pub fn with_max_cost_usd(self, usd: f64) -> Self {
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(rebuild_inner);
        Self {
            inner: Arc::new(Inner {
                max_cost_usd: Some(usd),
                ..inner
            }),
        }
    }

    /// Check all caps. Returns `Ok(())` if the loop should continue, or
    /// `Err(Trip)` with the reason the loop should stop.
    pub fn check(&self) -> Result<(), Trip> {
        if let Some(deadline) = self.inner.deadline {
            if Instant::now() >= deadline {
                return Err(Trip::Timeout);
            }
        }
        let c = self.inner.counters.lock();
        if let Some(max) = self.inner.max_steps {
            if c.steps >= max {
                return Err(Trip::MaxSteps);
            }
        }
        if let Some(max) = self.inner.max_tokens {
            if c.tokens >= max {
                return Err(Trip::MaxTokens);
            }
        }
        if let Some(max) = self.inner.max_cost_usd {
            if c.cost_usd >= max {
                return Err(Trip::MaxCostUsd);
            }
        }
        Ok(())
    }

    /// Increment the step counter.
    pub fn record_step(&self) {
        self.inner.counters.lock().steps += 1;
    }

    /// Add to the token counter.
    pub fn record_tokens(&self, n: u64) {
        self.inner.counters.lock().tokens += n;
    }

    /// Add to the USD cost counter.
    pub fn record_cost_usd(&self, usd: f64) {
        self.inner.counters.lock().cost_usd += usd;
    }

    /// Snapshot of the current counters: (steps, tokens, cost_usd).
    pub fn counters(&self) -> (usize, u64, f64) {
        let c = self.inner.counters.lock();
        (c.steps, c.tokens, c.cost_usd)
    }

    /// Time remaining before the deadline trips, if a timeout is set.
    pub fn time_remaining(&self) -> Option<Duration> {
        self.inner.deadline.map(|d| {
            let now = Instant::now();
            if now >= d {
                Duration::ZERO
            } else {
                d - now
            }
        })
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new()
    }
}

fn rebuild_inner(arc: Arc<Inner>) -> Inner {
    Inner {
        deadline: arc.deadline,
        max_steps: arc.max_steps,
        max_tokens: arc.max_tokens,
        max_cost_usd: arc.max_cost_usd,
        counters: Mutex::new(Counters::default()),
    }
}
