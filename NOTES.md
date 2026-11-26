# NOTES: Venue Shortlist

## 1. Spec and assumptions

**What the client said:** a partner day, mid-March, Thursday ideal and Tuesday OK. 80 people,
maybe 120. Near Utrecht Centraal or easy by train (the people come from Amsterdam and
Eindhoven). About €15k all-in, but catering *might* be a separate budget. One plenary room
plus 3–4 breakouts. Not dark, good wifi, wheelchair users attending.

**What I assume:**
- **Date:** March **2027** (the export is from Oct 2026 and quotes "March 2027"). Mid-March is
  8–19 March, so the candidates are Tue 9, Thu 11, Tue 16, Thu 18.
- **Hard requirements:** capacity ≥ headcount; ≥ 3 breakout rooms; wheelchair access; some
  daylight; wifi that isn't poor; ≤ 20 min walk from a station; a free Tue/Thu in March;
  venue cost ≤ €15k incl. VAT.
- **Money:** 21% VAT on prices quoted excl. VAT. A full day is 2 *dagdelen*. Catering, when
  not included, is an **assumed €40 pp** incl. VAT (the dataset's own notes quote €35–40) and
  is always labelled "assumed".
- **Source data:** the export says "unverified", so the output is a starting point for venue
  calls, not a booking confirmation.

**What I would ask Joa (the three questions):**
1. *Is catering inside the €15k or not?* This decides whether the budget gates on the venue
   cost or the all-in cost. At 120 people, 2 of the 7 venues that work for both (De
Seinwachter, Hub Moreelse) are over €15k all-in.
2. *Must wheelchair users reach the breakout rooms, or only the plenary?* This decides
   whether "ground floor only" (Pakhuis Catharijne) is a fail or a pass.
3. *Is a venue outside Utrecht acceptable if it is halfway for both groups (Den Bosch,
   Amersfoort)?* This decides whether location is a hard filter or only a score.

**Out of scope:** travel-time APIs, FX conversion, reading free-text `notes` for facts, a
config file, UI/API/DB.

## 2. Product decisions

**D1. 80 or 120 people?** The options were to plan for 80 (cheaper, more venues), to plan for
120 (safe), or to run both. **I chose both scenarios plus a "works for both" list** (7
venues). The client only knows in January. A venue that fits both means no rebooking, and
that list is the one I'd send first.

**D2. Unknown data: pass it, fail it, or ask?** Every check returns Pass, Unknown or Fail,
and venues land in *Shortlist*, *Needs confirmation* or *Excluded*. Unknown is never a pass
(the brief: don't treat unknown data as confirmed) and never a silent exclusion (a broken
capacity of `0` at De Graanbeurs shouldn't lose us a good venue). Each Unknown comes out as a
ready-to-send question for the venue.

**D3. Unclear wheelchair access.** `true` passes and `false` fails. Any qualifying text
("ground floor only", "lift to 1st floor, breakouts on 2nd") goes to *Needs confirmation*,
and the question to the venue quotes its own words back. I won't judge partial access on the
client's behalf.

**D4. Budget gate with an unclear catering budget.** The options were to gate on all-in, to
gate on venue cost, or to ignore catering. **I gate on the venue cost incl. VAT.** An all-in
figure above €15k is shown as a *risk*, and the ranking uses the all-in figure. Gating on
all-in would drop De Seinwachter (€15,058 all-in at 80) over an assumption we made ourselves.

**D5. Ranking when the price is missing.** A missing price gets 0 cost points and, because
it is Unknown on the budget check, can never reach the Shortlist. A USD price is treated as
missing; we never invent an exchange rate.

**D6. Duplicates.** The three pairs (Spoorhuis, Hotel Domzicht, Hub Moreelse) are merged
field by field. A known value beats an unknown one, and the *worse* known value wins (lower
capacity, the higher price). If the *only* reason a venue fails is a value its own sources
disagree on (Domzicht: 120 vs 110 seats), the result becomes a question, not an exclusion.

**D7. Location.** "Easy by train" is a hard limit of a ≤ 20 min walk. Being near Utrecht
Centraal is a score, not a filter. A venue in Amsterdam or Eindhoven scores 0 for location
(the home city of one group, far for the other), but can still be shortlisted.

## 3. Review

**How I steered:** I wrote and froze `.planning/REQUIREMENTS.md` (every rule numbered) before
any code. Then I **hand-computed the expected bucket for all 30 records** (`EXPECTED.md`) and
gave it to the Phase 2 agent as a test oracle. The agent was told to report disagreements,
not to "fix" the test. Phase 1 and Phase 2 were each built by a separate agent. I reviewed
each phase before committing, and the agent output is committed *unreviewed* so the diffs of
my fixes are visible.

**What I found:**
- **A silent data bug in the spec itself:** my own dry run showed a worst-value merge would
  quietly drop Hotel Domzicht at 120 because two sources disagree. I added rule D4 before
  Phase 2 started.
- **A last-year trap:** v004 *is* last year's venue ("Previous booking: partner day, March
  2026"). Its scraped duplicate v005 has no daylight or wifi data. A naive "pick the most
  complete record" or a "best value" merge would bring it back as *Needs confirmation*. The
  conservative merge keeps it excluded, and a test pins this.
- **Missing tests:** the agent's integration test asserted only buckets. I
  mutation-checked it: breaking the Dutch-thousands rule (`"7.900"`) left **every bucket
  assertion green**, because v008 stays in *Needs confirmation* either way. That is a silent
  data bug the tests could not see.
- **Over-engineering:** `score.rs` derived "the other day" from a `PREFERRED_DAY` const that
  nobody will ever change. `main.rs` cloned every evaluation to satisfy a function signature.
- **Accepted and credited:** the Phase 1 agent caught that "fully booked Q1 2027" would have
  been read as *tentative* by my rule N9. The Phase 2 agent made D4 stricter than I wrote it:
  it reruns the check on the best source value, so a venue whose sources say 60 vs 70 seats
  is still excluded.

**The diff I'm proudest of:** `tests/shortlist.rs::messy_prices_become_the_right_euro_amounts`.
It is five assertions, each pinning one messy field to an exact euro amount (Dutch
thousands, minimum spend + VAT, the higher duplicate price, no double VAT, no FX). It is
small, but it is the only test that fails when the money is wrong, and money errors are the
ones a client finds first.

**Hand edits (by me, not the agents):**
- `normalize/mod.rs`: the capacity range risk.
- `score.rs`: the day preference.
- `evaluate.rs`/`main.rs`: the slice signature.
- `tests/shortlist.rs`: the two new tests.
- All docs and planning files.

## 4. Next steps and time

- Simplify `dedupe.rs`: seven near-identical `merge_value` calls with three closures each.
  A `Display` impl plus a `worse_of` per field type would halve it (~30 min).
- Use the `notes` deliberately: "no public transport" (Heidezicht) and "catering via partner
  ~€40 pp" are facts we currently show but don't use (~1 h, with care not to over-parse).
- Travel time from Amsterdam *and* Eindhoven per station, to replace the coarse location
  tiers (~1 h, needs a data source).
- Send the three questions above and re-run once the client answers in January.

**Time spent:** about 3 hours in total. This includes installing a Rust toolchain on the
machine.
