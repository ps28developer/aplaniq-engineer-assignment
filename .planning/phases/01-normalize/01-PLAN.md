# Phase 1 plan: Load and normalize

## Tasks
1. `cargo init --name venue-shortlist` (edition 2021, lib plus bin). The only dependencies are
   `serde` (derive) and `serde_json`.
2. `src/raw.rs`: serde types that mirror the JSON exactly. Use `#[serde(untagged)]` enums for
   the polymorphic fields (capacity: number | string | map; breakout_rooms / walk_minutes:
   number | string; price: object | string; price.amount: number | string;
   wheelchair_accessible / catering_included: bool | string). Every field is `Option` and missing
   fields are tolerated. `RawFile { venues: Vec<serde_json::Value> }` lets each record be
   deserialized on its own (N10).
3. `src/model.rs`: the normalized domain types. Names must read like the business:
   `Venue`, `Capacity`, `Accessibility`, `Daylight`, `Wifi`, `Catering`, `Price`, `PriceUnit`,
   `Availability`, `Money` (i64 cents). Unknown is always an explicit variant.
4. `src/normalize/` with `mod.rs` (venue-level and simple fields), `price.rs` and `availability.rs`.
   Pure functions, one per field. Each returns the typed value plus any notes or risks it found.
5. `src/load.rs`: read the file and return `(Vec<Venue>, Vec<warning>)`. Error type via a small
   hand-written enum (no anyhow/thiserror).
6. Unit tests beside each parser, covering every form listed in REQUIREMENTS.md N1–N10.

## Done when
`cargo build`, `cargo test` and `cargo clippy -- -D warnings` are clean, and the acceptance
items in ROADMAP Phase 1 are met.
