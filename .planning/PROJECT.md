# Project: Venue Shortlist (Aplaniq engineer assignment)

## What
A small Rust CLI that reads `venues.json`, normalizes its messy fields, merges duplicate
listings, checks every venue against the client's hard requirements and prints a ranked,
explainable shortlist for two headcount scenarios (80 and 120).

## Why
Venue sourcing is the next roadmap item for Ace. This is the core that a future API handler
would call: deterministic, testable and honest about what the data does *not* say.

## Core value
A planner can send the output to the client today without having to re-check how a venue
was judged. Every exclusion has a reason, every score has a breakdown, and every unknown is
shown as a question for the venue, never as a pass.

## Constraints
- About 3 hours in total. Judgment over completeness.
- Rust, std plus `serde`/`serde_json` only. No UI, API, DB or deploy.
- One documented command (`cargo run -- venues.json`), and `cargo test` passes.
- Agents write the code. The human (tech lead) specifies, reviews and steers.

## Out of scope
Travel-time APIs, FX conversion, geocoding, a config file format, fuzzy-matching libraries,
date libraries (the only calendar needed is March 2027).
