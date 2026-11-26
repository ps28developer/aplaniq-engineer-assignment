# Venue Shortlist

A small Rust CLI. It reads a messy `venues.json`, normalizes it, merges duplicate listings,
checks each venue against the client's hard requirements and prints a ranked, explainable
shortlist for 80 and for 120 attendees.

Built for the Aplaniq engineer assignment. See [NOTES.md](NOTES.md) for the spec, the
decisions and the review, and [.planning/](.planning/) for the GSD planning artifacts.

## Setup
You need a stable Rust toolchain (1.75 or newer; developed on 1.100). Install it from
<https://rustup.rs>. The only dependencies are `serde` and `serde_json`.

## Run
```sh
cargo run -- venues.json
```
With no flags, the tool prints both scenarios (80 and 120 people) and the list of venues
that work for both. Options:

```sh
cargo run -- venues.json --headcount 120   # a single scenario
cargo run                                  # path defaults to ./venues.json
```

Each scenario has three sections:
- **Shortlist**: every hard requirement is confirmed by the data. Ranked by a 100-point
  score, with the breakdown shown.
- **Needs confirmation**: nothing fails, but something is missing or ambiguous. Lists the
  exact questions to ask the venue.
- **Excluded**: every reason the venue fails.

Malformed records are skipped with a warning on stderr. The exit code is non-zero only when
the arguments are bad or the file can't be read or parsed.

## Test
```sh
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```
The unit tests sit next to each module. `tests/shortlist.rs` runs the real `venues.json`
and asserts the bucket of every venue in both scenarios. The expected values come from a
hand-computed table in `.planning/phases/02-evaluate/EXPECTED.md`.

## Layout
| Module | Responsibility |
|---|---|
| `raw.rs` | serde types that mirror the JSON, including its polymorphic fields |
| `normalize/` | per-field parsers that turn raw data into typed values; unknown stays `Unknown` |
| `model.rs` | domain types (`Venue`, `Money` in cents, `Availability`, …) |
| `load.rs` | file I/O; each record is parsed independently |
| `dedupe.rs` | duplicate detection and a conservative field-by-field merge |
| `request.rs` | the client's requirements as data |
| `evaluate.rs` | hard requirement checks (Pass / Unknown / Fail), cost estimate, bucket |
| `score.rs` | transparent ranking weights |
| `report.rs` | plain-text output only |
