# Requirements (the contract handed to the implementing agents)

Event = partner day, **March 2027** (the dataset was exported Oct 2026 and refers to March 2027).
March 1, 2027 is a Monday, so Tue = 2, 9, 16, 23, 30 and Thu = 4, 11, 18, 25.
"Mid-March" window = 8–19 March, which gives the candidate dates Tue 9, Thu 11, Tue 16, Thu 18.

Each hard requirement check returns one of three outcomes:
- **Pass**: the data confirms it.
- **Unknown**: the data is missing or ambiguous. The venue goes to the *Needs confirmation*
  bucket, and the output names the question to ask. Unknown never counts as a pass.
- **Fail**: the data contradicts the requirement. The venue is *Excluded*, with the reason shown.

Venue bucket = Excluded if any check fails, else Needs confirmation if any check is Unknown,
else Shortlist. Venues are ranked by score within each bucket (ties broken by id).

## Normalization (N)
- **N1 capacity**: a positive integer is used as is. `0`, null or unparseable is **Unknown**
  (v027's 0 is a broken export; the notes' "up to 150" is unverified and not used).
  For a range `"80-120"`, the upper bound is the maximum. For `"90 seated / 130 standing"`,
  the seated number is used (a plenary is seated). For a layout object such as
  `{plenary_theatre, plenary_cabaret}`, the largest layout is used and the risk
  "capacity depends on layout" is recorded.
- **N2 breakout rooms**: an integer is used as is. For a range `"4-6"`, the lower bound is used.
  For `"3 (one is the lounge)"`, the leading integer is used and the parenthetical is kept as a note.
- **N3 walk minutes**: an integer, or the first integer in a string such as `"~10 min"`. Null is Unknown.
- **N4 price**: structured `{type, amount, currency, vat, minimum_spend?}` or free text.
  - The amount may be a number or a string. In a string, `.` followed by exactly 3 digits is a
    **Dutch thousands separator** (`"7.900"` = 7900, NOT 7.9). Same for `"€4.200 per dagdeel"`.
  - Units: `per_day`; `per_person`; `dagdeel` / day-part (a full day = 2 day-parts, recorded as an assumption).
  - VAT: `incl` or `excl`. Free text with "ex"/"excl" counts as excl. Missing VAT is treated as
    **excl**, because that is the conservative choice (it adds VAT), and a risk is recorded.
  - Non-EUR currency gives an **Unknown** price ("priced in USD, no FX conversion"). We never invent a rate.
  - Null, `"on request"` or unparseable text gives an Unknown price.
  - Money is stored as integer **euro cents**. VAT is 21%.
- **N5 catering_included**: `true` = Included, `false` = NotIncluded, `"partial"` = Partial, null = Unknown.
- **N6 wheelchair**: `true` = Yes, `false` = No, `"unknown"` = Unknown. Any other text is
  **Partial(text)**, because qualified access ("ground floor only", "breakouts on 2nd floor")
  needs confirmation.
- **N7 daylight** (checked in this order): partial/limited/mixed = Partial; "no", "no windows",
  "black box" = None; starts with "yes" = Yes; null = Unknown; unrecognized text = Unknown.
- **N8 wifi** (checked in this order): weak/complaint/4g/poor/bad/terrible = Poor;
  fiber/gbps/mbps/good/dedicated = Good; guest/fair/basic = Basic; null or unrecognized = Unknown.
- **N9 availability**: free text becomes the set of March 2027 Tue/Thu dates, plus a `tentative` flag.
  - "ask", "unknown", empty or null gives **Unknown**.
  - "fully booked" without "except" gives an empty set. "fully booked except 9 and 11 March"
    gives {9, 11} (filtered to Tue/Thu).
  - Explicit day numbers ("Tue 9, 16, 23 March; Thu 11, 25 March") give exactly those dates.
    A number followed by "week(s)" is not a date ("book 6 weeks ahead").
  - Otherwise, weekday words select the dates: tue*, thu*, "weekdays"/"mon-fri" (both days).
    "except N" removes N.
  - `tentative = true` when the text says "most", or when it does not mention March at all
    (e.g. "Tuesdays").
- **N10 robustness**: each venue is deserialized on its own. A malformed record is reported as
  a warning and skipped, and never aborts the run.

## Duplicates (D)
- **D1**: Two records are the same venue when their *name keys* and their cities match. The name
  key is built by lowercasing the name, dropping any parenthetical, dropping everything after a
  spaced dash (`" — "`, `" – "`, `" - "`), dropping the articles het/de/the and the tokens of the
  city name, and keeping only alphanumerics. Expected groups in the data: v004+v005, v009+v010, v023+v024.
- **D2 merge, field by field and conservative**: a known value beats an unknown one. When two
  known values conflict, the worse one is kept for the client and a `data conflict` note is recorded:
  min capacity, min breakouts, max walk, worst wheelchair/daylight/wifi, intersection of
  availability dates, and the *highest* price estimate (all known prices are kept, and the cost
  is the max over them).
- **D3**: The output lists all source ids of a merged venue.

## Hard requirements (R), per headcount scenario
- **R1 capacity** ≥ headcount.
- **R2 breakout rooms** ≥ 3 (the client asked for 3–4).
- **R3 wheelchair**: Yes = Pass, No = Fail, Partial/Unknown = Unknown.
- **R4 daylight**: None = Fail ("last year was too dark"), Unknown = Unknown, Partial = Pass with a risk.
- **R5 wifi**: Poor = Fail ("wifi was terrible"), Unknown = Unknown, Basic = Pass with a risk.
- **R6 easy by train**: walk from the nearest station ≤ **20 min**, otherwise Fail. Unknown walk = Unknown.
- **R7 availability**: Pass if at least one Tue/Thu in March 2027 is listed. An empty set = Fail.
  Unknown or tentative = Unknown.
- **R8 budget**: the **venue cost incl. VAT, excluding the catering estimate** must be ≤ €15,000.
  The client is unsure whether catering has its own budget, so the all-in figure is not a hard
  gate. When the all-in estimate exceeds €15k, a risk is recorded instead. An Unknown price = Unknown.

## Cost estimate (C)
- venue cost = per_day amount; or day-part amount × 2; or per_person amount × headcount, raised
  to the minimum spend if lower. Converted to incl. VAT (×1.21 when excl).
- catering estimate = €0 when catering is Included. Otherwise it is an **assumed €40 pp incl. VAT**
  (the dataset's own notes quote €35–40 pp). It is always labeled "assumed" in the output.
- all-in = venue + catering estimate.

## Scoring (S): 100 points, transparent, constants in one place
Only non-excluded venues are scored. The output shows each component.
| Component | Max | Rule |
|---|---|---|
| Cost | 35 | 35 × (1 − all_in / 15000), clamped to 0..35. Unknown price = 0 |
| Location | 25 | Utrecht Centraal & walk ≤ 10 = 25; Utrecht Centraal & walk ≤ 20 = 20; other Utrecht station = 15; other city = 8; Amsterdam/Eindhoven station (home city of one group, far for the other) = 0 |
| Availability | 15 | Thursday in mid-March window = 15; Tuesday in window = 10; only outside the window = 5; unknown = 0 |
| Daylight | 10 | Yes 10, Partial 4, Unknown 0 |
| Wifi | 10 | Good 10, Basic 4, Unknown 0 |
| Breakouts | 5 | ≥ 4 = 5, 3 = 3 |

## Output (O)
- **O1**: By default both scenarios (80 and 120) are printed. `--headcount N` runs a single scenario.
- **O2**: Per venue: rank, name, ids, score with its breakdown, venue cost and all-in estimate
  (with the catering marked as assumed when it is), the candidate dates, risks, and the open
  questions for the venue.
- **O3**: Excluded venues are listed with every failing reason.
- **O4**: A closing "works for both 80 and 120" list (shortlisted in both scenarios). It answers
  the client's open headcount question.
- **O5**: `cargo run -- venues.json` is the one command. The exit code is non-zero only when the
  file is unreadable or not JSON.
