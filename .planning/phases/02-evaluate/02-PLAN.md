# Phase 2 plan: Dedupe, evaluate, score, report

## Tasks
1. `src/dedupe.rs`: `name_key`, `group_duplicates`, `merge` (D1–D3). Tests: the three real
   pairs merge; distinct venues do not; conflicts are recorded; known beats unknown.
2. `src/request.rs`: `EventRequest` holds the client's hard requirements as data (headcount,
   budget, min breakouts, max walk, window, preferred days). The constructor is
   `EventRequest::partner_day(headcount)`.
3. `src/evaluate.rs`: `Check { requirement, outcome: Pass | Unknown(question) | Fail(reason) }`,
   `CostEstimate`, `Evaluation { venue, checks, bucket, cost, risks, score }` (R1–R8, C).
4. `src/score.rs`: `ScoreBreakdown` with one field per component, weights as `const`s (S).
5. `src/report.rs`: plain-text rendering (O1–O4). There is no logic in here beyond formatting.
6. `src/main.rs`: hand-rolled argument parsing (`[PATH] [--headcount N]`), default path
   `venues.json`. Calls the lib and prints.
7. `tests/shortlist.rs`: an integration test against the real `venues.json`.

## Done when
The ROADMAP Phase 2 acceptance criteria are met and clippy is clean.
