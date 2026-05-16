# agent-watchdog

[![crates.io](https://img.shields.io/crates/v/agent-watchdog.svg)](https://crates.io/crates/agent-watchdog)
[![docs.rs](https://docs.rs/agent-watchdog/badge.svg)](https://docs.rs/agent-watchdog)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Deadline + budget watchdog for LLM agent loops. Stop runaway agents cleanly with a structured reason.

```toml
[dependencies]
agent-watchdog = "0.1"
```

## Why

Agent loops drift when nothing kills them. Most of my "the agent went weird" tickets trace to one of four failures:

- It looped past the step budget you intended.
- It blew the per-task wall-clock budget.
- It spent way more tokens than the per-task budget would have allowed.
- It cost way more in USD than the per-task budget would have allowed.

Each one is the same shape: a counter, a cap, a clean stop. `agent-watchdog` is that primitive, in 200 lines.

## Quick start

```rust
use agent_watchdog::{Watchdog, Trip};
use std::time::Duration;

let wd = Watchdog::new()
    .with_timeout(Duration::from_secs(30))
    .with_max_steps(10)
    .with_max_tokens(20_000)
    .with_max_cost_usd(0.50);

loop {
    match wd.check() {
        Ok(()) => { /* run one agent step */ }
        Err(Trip::Timeout)    => { log::warn!("agent timed out"); break; }
        Err(Trip::MaxSteps)   => { log::warn!("agent hit step cap"); break; }
        Err(Trip::MaxTokens)  => { log::warn!("agent hit token cap"); break; }
        Err(Trip::MaxCostUsd) => { log::warn!("agent hit cost cap"); break; }
    }

    let resp = call_model().await;
    wd.record_step();
    wd.record_tokens(resp.usage.input_tokens + resp.usage.output_tokens);
    wd.record_cost_usd(compute_cost(&resp));
}
```

## Composes with the rest of the agent stack

Pair with `claude-cost` / `openai-cost` / `gemini-cost` / `bedrock-cost` for the cost number, with `cachebench` for the token usage block, with `agenttrace` for the run record:

```rust
let usd  = claude_cost::compute(&resp.usage, &claude_cost::CLAUDE_SONNET_4);
wd.record_cost_usd(usd);
wd.record_tokens(resp.usage.input_tokens + resp.usage.output_tokens);
```

`Watchdog` is `Clone`; clones share the same counters, so you can hand a copy to a background telemetry task and it sees the same accumulating budget.

## Priority

When multiple caps could trip, the order is:

1. `Timeout` (most external; wall-clock is the boss)
2. `MaxSteps`
3. `MaxTokens`
4. `MaxCostUsd`

This is so log lines tell you *the most useful* reason. Wall-clock trumps everything because it's the cleanest signal.

## What it doesn't do

- It does **not** cancel an in-flight HTTP request. That's transport-layer concern; use `tokio::select!` with `wd.time_remaining()` if you need cancel semantics.
- It does **not** count tokens for you. Pair with the tokenizer of your choice.
- It does **not** persist counters across process restarts.

## Sibling

[`token-budget-pool`](https://crates.io/crates/token-budget-pool) is the shared budget across concurrent tasks. `agent-watchdog` is the single-loop watchdog. Use one or both depending on whether you have one agent or a fleet.

## License

MIT
