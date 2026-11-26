# Roadmap

## Phase 1: Load and normalize
Goal: turn every raw record into a strongly typed `Venue` without inventing data.
Requirements: N1–N10.
Acceptance:
- [ ] `cargo test` covers every messy form that appears in `venues.json` (one test per form).
- [ ] `"7.900"` parses as 7900 euros. v027 capacity 0 becomes Unknown. USD becomes an Unknown price.
- [ ] A malformed venue record gives a warning and does not abort the run.

## Phase 2: Dedupe, evaluate, score, report
Goal: a ranked, explainable shortlist per scenario.
Requirements: D1–D3, R1–R8, C, S, O1–O5.
Acceptance:
- [ ] The three duplicate groups merge. The merged Spoorhuis stays **excluded** (dark, bad wifi),
      even though v005 alone has only unknowns.
- [ ] An integration test runs the real `venues.json` and asserts the bucket of key venues per scenario.
- [ ] `cargo run -- venues.json` prints both scenarios and the "works for both" list.

## Phase 3: Independent review and hardening
Goal: a senior review of the agent output. Fix logic bugs, naming, dead code and test gaps.
Acceptance:
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` are all clean.
- [ ] The review findings are logged in NOTES.md, including the proudest diff.

## Phase 4: Docs
README.md (setup/run/test) and NOTES.md (≤ 2 pages).
