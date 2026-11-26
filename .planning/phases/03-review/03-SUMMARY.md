# Phase 3 summary: independent review

| Finding | Kind | Action |
|---|---|---|
| A worst-value merge silently excludes Hotel Domzicht at 120 (sources say 120 vs 110) | silent data bug (spec) | Rule D4 added before Phase 2. Implemented and tested |
| The capacity range "80-120" used its upper bound with no warning | silent optimism | A range risk was added (hand edit) |
| "fully booked Q1 2027" would have been tentative under N9 | spec bug (found by the agent) | Accepted. N9 clarified |
| The integration test only asserted buckets. Breaking the Dutch-thousands rule kept it green | missing test | Mutation-checked. Exact-cost and determinism tests added |
| `main.rs` cloned all evaluations to call `shortlisted_in_all` | needless work | The signature now takes slices |
| `score.rs` derived the "other day" from a const nobody flips | over-engineering | Written as the brief states it |
| `dedupe.rs` has seven near-identical merge blocks | verbosity | Deferred (NOTES §4) |
