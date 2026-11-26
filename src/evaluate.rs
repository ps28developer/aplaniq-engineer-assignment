//! R1–R8 and C: checks every venue against the brief. A check only passes on confirmed data;
//! missing or ambiguous data becomes a question for the venue, never a silent pass.

use std::cmp::Ordering;
use std::fmt;

use crate::dedupe::merge_duplicates;
use crate::model::{
    Accessibility, Availability, BreakoutRooms, Capacity, Catering, Daylight, Disputed,
    DisputedField, Money, Price, PriceUnit, Venue, WalkMinutes, Wifi,
};
use crate::request::{day_label, EventRequest};
use crate::score::{location, score, Location, ScoreBreakdown};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Capacity,
    BreakoutRooms,
    Wheelchair,
    Daylight,
    Wifi,
    EasyByTrain,
    Availability,
    Budget,
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Requirement::Capacity => "R1 capacity",
            Requirement::BreakoutRooms => "R2 breakout rooms",
            Requirement::Wheelchair => "R3 wheelchair access",
            Requirement::Daylight => "R4 daylight",
            Requirement::Wifi => "R5 wifi",
            Requirement::EasyByTrain => "R6 easy by train",
            Requirement::Availability => "R7 availability",
            Requirement::Budget => "R8 budget",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    /// The question to ask the venue.
    Unknown(String),
    /// Why the venue is excluded.
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub requirement: Requirement,
    pub outcome: Outcome,
}

/// Declared in ranking order: the shortlist comes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bucket {
    Shortlist,
    NeedsConfirmation,
    Excluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostEstimate {
    /// The budget gate (R8): catering may have its own budget, so it is not in here.
    pub venue_incl_vat: Money,
    pub catering: Money,
    /// The catering figure is our €40 pp estimate, not a venue quote.
    pub catering_assumed: bool,
    pub all_in: Money,
}

#[derive(Debug, Clone)]
pub struct Evaluation {
    pub venue: Venue,
    pub checks: Vec<Check>,
    pub bucket: Bucket,
    /// `None` when no price is known in EUR.
    pub cost: Option<CostEstimate>,
    pub risks: Vec<String>,
    /// Only shortlisted and needs-confirmation venues are scored.
    pub score: Option<ScoreBreakdown>,
}

impl Evaluation {
    pub fn questions(&self) -> impl Iterator<Item = &str> {
        self.checks.iter().filter_map(|c| match &c.outcome {
            Outcome::Unknown(question) => Some(question.as_str()),
            _ => None,
        })
    }

    pub fn fail_reasons(&self) -> impl Iterator<Item = (Requirement, &str)> {
        self.checks.iter().filter_map(|c| match &c.outcome {
            Outcome::Fail(reason) => Some((c.requirement, reason.as_str())),
            _ => None,
        })
    }
}

/// Merges duplicates, evaluates every venue and ranks them: by bucket, then score, then id.
pub fn shortlist(venues: &[Venue], request: &EventRequest) -> Vec<Evaluation> {
    let mut evaluations: Vec<Evaluation> = merge_duplicates(venues.to_vec())
        .iter()
        .map(|venue| evaluate(venue, request))
        .collect();
    evaluations.sort_by(rank_order);
    evaluations
}

fn rank_order(a: &Evaluation, b: &Evaluation) -> Ordering {
    let total = |e: &Evaluation| e.score.map_or(0.0, |s| s.total());
    a.bucket
        .cmp(&b.bucket)
        .then_with(|| total(b).total_cmp(&total(a)))
        .then_with(|| a.venue.ids[0].cmp(&b.venue.ids[0]))
}

/// The venues shortlisted in every scenario, in the order of the first one. This answers the
/// client's open headcount question.
pub fn shortlisted_in_all(scenarios: &[Vec<Evaluation>]) -> Vec<&Venue> {
    let Some((first, rest)) = scenarios.split_first() else {
        return Vec::new();
    };
    first
        .iter()
        .filter(|e| e.bucket == Bucket::Shortlist)
        .filter(|e| {
            rest.iter().all(|scenario| {
                scenario
                    .iter()
                    .any(|o| o.venue.ids == e.venue.ids && o.bucket == Bucket::Shortlist)
            })
        })
        .map(|e| &e.venue)
        .collect()
}

pub fn evaluate(venue: &Venue, request: &EventRequest) -> Evaluation {
    let (cost, cost_risks) = match estimate_cost(&venue.prices, venue.catering, request) {
        Some((cost, risks)) => (Some(cost), risks),
        None => (None, Vec::new()),
    };

    let station = venue
        .nearest_station
        .as_deref()
        .unwrap_or("the nearest station");
    let mut checks = vec![
        Check {
            requirement: Requirement::Capacity,
            outcome: check_capacity(venue.capacity, request),
        },
        Check {
            requirement: Requirement::BreakoutRooms,
            outcome: check_breakout_rooms(venue.breakout_rooms, request),
        },
        Check {
            requirement: Requirement::Wheelchair,
            outcome: check_wheelchair(&venue.wheelchair),
        },
        Check {
            requirement: Requirement::Daylight,
            outcome: check_daylight(venue.daylight),
        },
        Check {
            requirement: Requirement::Wifi,
            outcome: check_wifi(venue.wifi, request),
        },
        Check {
            requirement: Requirement::EasyByTrain,
            outcome: check_walk(venue.walk_minutes, station, request),
        },
        Check {
            requirement: Requirement::Availability,
            outcome: check_availability(&venue.availability),
        },
        Check {
            requirement: Requirement::Budget,
            outcome: check_budget(cost.as_ref(), venue, request),
        },
    ];
    for disputed in &venue.disputed {
        apply_dispute(&mut checks, disputed, station, request);
    }

    let bucket = if checks.iter().any(|c| matches!(c.outcome, Outcome::Fail(_))) {
        Bucket::Excluded
    } else if checks
        .iter()
        .any(|c| matches!(c.outcome, Outcome::Unknown(_)))
    {
        Bucket::NeedsConfirmation
    } else {
        Bucket::Shortlist
    };

    let risks = collect_risks(venue, cost.as_ref(), cost_risks, request);
    let score = (bucket != Bucket::Excluded).then(|| score(venue, cost.as_ref(), request));
    Evaluation {
        venue: venue.clone(),
        checks,
        bucket,
        cost,
        risks,
        score,
    }
}

/// D4: a fail that the best source value would not produce is caused only by the dispute, so
/// it becomes a question. A pass already uses the worse value, so it is never upgraded.
fn apply_dispute(checks: &mut [Check], disputed: &Disputed, station: &str, request: &EventRequest) {
    let (requirement, best_case) = match &disputed.field {
        DisputedField::Capacity { best } => (
            Requirement::Capacity,
            check_capacity(Capacity::Known(*best), request),
        ),
        DisputedField::BreakoutRooms { best } => (
            Requirement::BreakoutRooms,
            check_breakout_rooms(BreakoutRooms::Known(*best), request),
        ),
        DisputedField::WalkMinutes { best } => (
            Requirement::EasyByTrain,
            check_walk(WalkMinutes::Known(*best), station, request),
        ),
        DisputedField::Wheelchair { best } => (Requirement::Wheelchair, check_wheelchair(best)),
        DisputedField::Daylight { best } => (Requirement::Daylight, check_daylight(*best)),
        DisputedField::Wifi { best } => (Requirement::Wifi, check_wifi(*best, request)),
        DisputedField::Availability { best } => {
            (Requirement::Availability, check_availability(best))
        }
        DisputedField::Catering | DisputedField::Price => return,
    };
    if matches!(best_case, Outcome::Fail(_)) {
        return;
    }
    for check in checks.iter_mut().filter(|c| c.requirement == requirement) {
        if let Outcome::Fail(reason) = &check.outcome {
            check.outcome = Outcome::Unknown(format!(
                "sources disagree: {}. Which is correct? (the lower figure fails: {reason})",
                disputed.detail
            ));
        }
    }
}

/// R1.
fn check_capacity(capacity: Capacity, request: &EventRequest) -> Outcome {
    let headcount = request.headcount;
    match capacity {
        Capacity::Known(n) if n >= headcount => Outcome::Pass,
        Capacity::Known(n) => Outcome::Fail(format!("capacity {n} is below {headcount}")),
        Capacity::Unknown => Outcome::Unknown(format!(
            "How many people does the plenary seat? The listing has no usable capacity and we need {headcount}."
        )),
    }
}

/// R2.
fn check_breakout_rooms(rooms: BreakoutRooms, request: &EventRequest) -> Outcome {
    let min = request.min_breakout_rooms;
    match rooms {
        BreakoutRooms::Known(n) if n >= min => Outcome::Pass,
        BreakoutRooms::Known(n) => {
            Outcome::Fail(format!("only {n} breakout rooms (at least {min} needed)"))
        }
        BreakoutRooms::Unknown => Outcome::Unknown(format!(
            "How many breakout rooms are there? We need at least {min}."
        )),
    }
}

/// R3. Qualified access is quoted back to the venue, because "ground floor only" may still
/// leave the breakout rooms out of reach.
fn check_wheelchair(access: &Accessibility) -> Outcome {
    match access {
        Accessibility::Yes => Outcome::Pass,
        Accessibility::No => Outcome::Fail("not wheelchair accessible".to_string()),
        Accessibility::Partial(text) => Outcome::Unknown(format!(
            "Your listing says \"{text}\". Can a wheelchair user reach the plenary and every breakout room?"
        )),
        Accessibility::Unknown => Outcome::Unknown(
            "Is the venue wheelchair accessible, breakout rooms included? The listing does not say."
                .to_string(),
        ),
    }
}

/// R4. Last year's venue was too dark, so no daylight is a hard fail.
fn check_daylight(daylight: Daylight) -> Outcome {
    match daylight {
        Daylight::Yes | Daylight::Partial => Outcome::Pass,
        Daylight::None => {
            Outcome::Fail("no daylight in the plenary (last year was too dark)".to_string())
        }
        Daylight::Unknown => Outcome::Unknown("Does the plenary room have daylight?".to_string()),
    }
}

/// R5. Last year's wifi was terrible, so poor wifi is a hard fail.
fn check_wifi(wifi: Wifi, request: &EventRequest) -> Outcome {
    match wifi {
        Wifi::Good | Wifi::Basic => Outcome::Pass,
        Wifi::Poor => Outcome::Fail("poor wifi (last year's wifi was terrible)".to_string()),
        Wifi::Unknown => Outcome::Unknown(format!(
            "What wifi do you offer, and does it hold {} people?",
            request.headcount
        )),
    }
}

/// R6.
fn check_walk(walk: WalkMinutes, station: &str, request: &EventRequest) -> Outcome {
    let max = request.max_walk_minutes;
    match walk {
        WalkMinutes::Known(m) if m <= max => Outcome::Pass,
        WalkMinutes::Known(m) => Outcome::Fail(format!("{m} min walk from {station} (max {max})")),
        WalkMinutes::Unknown => Outcome::Unknown(format!(
            "How many minutes is the walk from {station}? We need {max} or less."
        )),
    }
}

/// R7. A hedged listing ("most Thursdays", no month named) is a question, not a date.
fn check_availability(availability: &Availability) -> Outcome {
    match availability {
        Availability::Unknown => {
            Outcome::Unknown("Which Tuesdays and Thursdays in March 2027 are free?".to_string())
        }
        Availability::Listed { dates, tentative } if dates.is_empty() => {
            if *tentative {
                Outcome::Unknown("Are any Tuesdays or Thursdays in March 2027 free?".to_string())
            } else {
                Outcome::Fail("no Tuesday or Thursday free in March 2027".to_string())
            }
        }
        Availability::Listed {
            dates,
            tentative: true,
        } => {
            let days: Vec<String> = dates.iter().map(|d| day_label(*d)).collect();
            Outcome::Unknown(format!(
                "Can you confirm which of these March 2027 dates are free: {}?",
                days.join(", ")
            ))
        }
        Availability::Listed { .. } => Outcome::Pass,
    }
}

/// R8. Only the venue cost is gated; the all-in figure is a risk because catering may be
/// paid from a separate budget.
fn check_budget(cost: Option<&CostEstimate>, venue: &Venue, request: &EventRequest) -> Outcome {
    match cost {
        Some(cost) if cost.venue_incl_vat <= request.budget => Outcome::Pass,
        Some(cost) => Outcome::Fail(format!(
            "venue {} incl. VAT is over the {} budget",
            cost.venue_incl_vat, request.budget
        )),
        None => Outcome::Unknown(format!(
            "What is the day rate for {} people, in EUR incl. VAT? ({})",
            request.headcount,
            venue
                .price_unknown_reason
                .as_deref()
                .unwrap_or("price not stated")
        )),
    }
}

/// C. Returns `None` without a known price, plus the assumptions behind the figure.
fn estimate_cost(
    prices: &[Price],
    catering: Catering,
    request: &EventRequest,
) -> Option<(CostEstimate, Vec<String>)> {
    let mut risks = Vec::new();
    let (venue_incl_vat, minimum_spend_note) = prices
        .iter()
        .map(|price| venue_cost(price, request.headcount))
        .max_by_key(|(cost, _)| *cost)?;
    risks.extend(minimum_spend_note);

    let assumed = request
        .catering_estimate_per_person
        .times(request.headcount);
    let (catering_cost, catering_assumed) = match catering {
        Catering::Included => (Money::from_cents(0), false),
        Catering::NotIncluded | Catering::Partial | Catering::Unknown => (assumed, true),
    };
    let cost = CostEstimate {
        venue_incl_vat,
        catering: catering_cost,
        catering_assumed,
        all_in: venue_incl_vat + catering_cost,
    };
    Some((cost, risks))
}

/// The venue cost incl. VAT for one price, and a note when the minimum spend raised it.
pub fn venue_cost(price: &Price, headcount: u32) -> (Money, Option<String>) {
    let (cost, note) = match price.unit {
        PriceUnit::PerDay => (price.amount, None),
        PriceUnit::PerDayPart => (price.amount.times(2), None),
        PriceUnit::PerPerson => {
            let per_head = price.amount.times(headcount);
            match price.minimum_spend {
                Some(minimum) if minimum > per_head => {
                    let note = format!(
                        "minimum spend {minimum} applies ({headcount} x {} = {per_head})",
                        price.amount
                    );
                    (minimum, Some(note))
                }
                _ => (per_head, None),
            }
        }
    };
    let incl = if price.vat_included {
        cost
    } else {
        cost.add_vat()
    };
    (incl, note)
}

fn collect_risks(
    venue: &Venue,
    cost: Option<&CostEstimate>,
    cost_risks: Vec<String>,
    request: &EventRequest,
) -> Vec<String> {
    let mut risks = venue.risks.clone();
    for disputed in &venue.disputed {
        let used = match disputed.field {
            DisputedField::Price => "highest used",
            _ => "worse value used",
        };
        risks.push(format!("data conflict: {} ({used})", disputed.detail));
    }
    if venue.daylight == Daylight::Partial {
        risks.push("partial daylight only".to_string());
    }
    if venue.wifi == Wifi::Basic {
        risks.push(format!(
            "basic wifi only: check it holds {} people",
            request.headcount
        ));
    }
    risks.extend(cost_risks);
    if let Some(cost) = cost {
        if cost.catering_assumed {
            risks.push(format!(
                "catering {} assumed ({} pp, not a venue quote)",
                cost.catering, request.catering_estimate_per_person
            ));
        }
        if cost.all_in > request.budget {
            risks.push(format!(
                "over {} all-in ({}) if catering comes from the same budget",
                request.budget, cost.all_in
            ));
        }
    }
    match venue.catering {
        Catering::Partial => {
            risks.push("catering only partly included: ask what is covered".to_string())
        }
        Catering::Unknown => risks.push("catering not stated: ask if it is included".to_string()),
        Catering::Included | Catering::NotIncluded => {}
    }
    let station = venue.nearest_station.as_deref().unwrap_or("unknown");
    match location(venue.nearest_station.as_deref()) {
        Location::UtrechtCentraal | Location::OtherUtrecht => {}
        Location::OriginCity => risks.push(format!(
            "outside Utrecht ({station}): home city of one group, far for the other"
        )),
        Location::OtherCity => risks.push(format!("outside Utrecht ({station})")),
        Location::Unknown => risks.push("outside Utrecht? nearest station not stated".to_string()),
    }
    risks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::parse_venues;
    use crate::model::Money;

    fn real_venues() -> Vec<Venue> {
        parse_venues(include_str!("../venues.json")).unwrap().venues
    }

    fn find<'a>(evaluations: &'a [Evaluation], id: &str) -> &'a Evaluation {
        evaluations
            .iter()
            .find(|e| e.venue.ids[0] == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    }

    fn outcome(evaluation: &Evaluation, requirement: Requirement) -> &Outcome {
        &evaluation
            .checks
            .iter()
            .find(|c| c.requirement == requirement)
            .unwrap()
            .outcome
    }

    fn price(euros: i64, unit: PriceUnit, vat_included: bool) -> Price {
        Price {
            amount: Money::from_euros(euros),
            unit,
            vat_included,
            minimum_spend: None,
        }
    }

    fn is_unknown(outcome: &Outcome) -> bool {
        matches!(outcome, Outcome::Unknown(_))
    }

    fn is_fail(outcome: &Outcome) -> bool {
        matches!(outcome, Outcome::Fail(_))
    }

    #[test]
    fn disputed_capacity_is_unknown_for_120_and_passes_for_80() {
        let venues = real_venues();
        let at_80 = shortlist(&venues, &EventRequest::partner_day(80));
        let at_120 = shortlist(&venues, &EventRequest::partner_day(120));
        let domzicht_80 = find(&at_80, "v009");
        let domzicht_120 = find(&at_120, "v009");
        assert_eq!(outcome(domzicht_80, Requirement::Capacity), &Outcome::Pass);
        assert_eq!(domzicht_80.bucket, Bucket::Shortlist);
        match outcome(domzicht_120, Requirement::Capacity) {
            Outcome::Unknown(q) => assert!(q.contains("capacity: 120 vs 110"), "{q}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(domzicht_120.bucket, Bucket::NeedsConfirmation);
    }

    #[test]
    fn dispute_where_every_source_fails_stays_a_fail() {
        let mut venue = real_venues().remove(0);
        venue.capacity = Capacity::Known(60);
        venue.disputed.push(Disputed {
            field: DisputedField::Capacity { best: 70 },
            detail: "capacity: 70 vs 60".to_string(),
        });
        let evaluation = evaluate(&venue, &EventRequest::partner_day(80));
        assert!(is_fail(outcome(&evaluation, Requirement::Capacity)));
    }

    #[test]
    fn merged_spoorhuis_stays_excluded() {
        let evaluations = shortlist(&real_venues(), &EventRequest::partner_day(80));
        let spoorhuis = find(&evaluations, "v004");
        assert_eq!(spoorhuis.venue.ids, vec!["v004", "v005"]);
        assert_eq!(spoorhuis.bucket, Bucket::Excluded);
        assert!(is_fail(outcome(spoorhuis, Requirement::Daylight)));
        assert!(is_fail(outcome(spoorhuis, Requirement::Wifi)));
        assert!(spoorhuis.score.is_none());
    }

    #[test]
    fn minimum_spend_raises_the_venue_cost() {
        let mut p = price(95, PriceUnit::PerPerson, false);
        p.minimum_spend = Some(Money::from_euros(10_000));
        let (cost, note) = venue_cost(&p, 80);
        assert_eq!(cost, Money::from_euros(12_100));
        assert!(note.unwrap().starts_with("minimum spend"));
        // At 120 people the per-person total is above the minimum spend.
        let (cost, note) = venue_cost(&p, 120);
        assert_eq!(cost, Money::from_euros(11_400).add_vat());
        assert_eq!(note, None);
    }

    #[test]
    fn vat_is_added_only_to_excl_prices() {
        assert_eq!(
            venue_cost(&price(9800, PriceUnit::PerDay, false), 80).0,
            Money::from_euros(11_858)
        );
        assert_eq!(
            venue_cost(&price(7200, PriceUnit::PerDay, true), 80).0,
            Money::from_euros(7200)
        );
    }

    #[test]
    fn day_part_counts_twice_for_a_full_day() {
        assert_eq!(
            venue_cost(&price(4200, PriceUnit::PerDayPart, false), 80).0,
            Money::from_euros(8400).add_vat()
        );
    }

    #[test]
    fn per_person_scales_with_headcount() {
        assert_eq!(
            venue_cost(&price(55, PriceUnit::PerPerson, true), 120).0,
            Money::from_euros(6600)
        );
    }

    #[test]
    fn cost_uses_the_highest_known_price() {
        let prices = [
            price(8900, PriceUnit::PerDay, false),
            price(9400, PriceUnit::PerDay, false),
        ];
        let (cost, _) =
            estimate_cost(&prices, Catering::Included, &EventRequest::partner_day(80)).unwrap();
        assert_eq!(cost.venue_incl_vat, Money::from_euros(11_374));
        assert_eq!(cost.all_in, Money::from_euros(11_374));
        assert!(!cost.catering_assumed);
    }

    #[test]
    fn catering_is_assumed_unless_included() {
        let prices = [price(1000, PriceUnit::PerDay, true)];
        let request = EventRequest::partner_day(80);
        for catering in [Catering::NotIncluded, Catering::Partial, Catering::Unknown] {
            let (cost, _) = estimate_cost(&prices, catering, &request).unwrap();
            assert_eq!(cost.catering, Money::from_euros(3200), "{catering:?}");
            assert!(cost.catering_assumed);
            assert_eq!(cost.all_in, Money::from_euros(4200));
        }
        assert!(estimate_cost(&[], Catering::Included, &request).is_none());
    }

    #[test]
    fn budget_gate_uses_venue_cost_and_all_in_is_only_a_risk() {
        let evaluations = shortlist(&real_venues(), &EventRequest::partner_day(80));
        let seinwachter = find(&evaluations, "v001");
        let cost = seinwachter.cost.unwrap();
        assert_eq!(cost.venue_incl_vat, Money::from_euros(11_858));
        assert_eq!(cost.all_in, Money::from_euros(15_058));
        assert_eq!(outcome(seinwachter, Requirement::Budget), &Outcome::Pass);
        assert_eq!(seinwachter.bucket, Bucket::Shortlist);
        assert!(seinwachter
            .risks
            .iter()
            .any(|r| r.starts_with("over €15,000.00 all-in")));

        let zeeburg = find(&evaluations, "v013");
        assert!(is_fail(outcome(zeeburg, Requirement::Budget)));
    }

    #[test]
    fn capacity_outcomes() {
        let request = EventRequest::partner_day(80);
        assert_eq!(check_capacity(Capacity::Known(80), &request), Outcome::Pass);
        assert!(is_fail(&check_capacity(Capacity::Known(79), &request)));
        assert!(is_unknown(&check_capacity(Capacity::Unknown, &request)));
    }

    #[test]
    fn breakout_outcomes() {
        let request = EventRequest::partner_day(80);
        assert_eq!(
            check_breakout_rooms(BreakoutRooms::Known(3), &request),
            Outcome::Pass
        );
        assert!(is_fail(&check_breakout_rooms(
            BreakoutRooms::Known(2),
            &request
        )));
        assert!(is_unknown(&check_breakout_rooms(
            BreakoutRooms::Unknown,
            &request
        )));
    }

    #[test]
    fn wheelchair_outcomes_quote_the_venue() {
        assert_eq!(check_wheelchair(&Accessibility::Yes), Outcome::Pass);
        assert!(is_fail(&check_wheelchair(&Accessibility::No)));
        assert!(is_unknown(&check_wheelchair(&Accessibility::Unknown)));
        match check_wheelchair(&Accessibility::Partial("ground floor only".into())) {
            Outcome::Unknown(q) => assert!(q.contains("\"ground floor only\""), "{q}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn daylight_outcomes() {
        assert_eq!(check_daylight(Daylight::Yes), Outcome::Pass);
        assert_eq!(check_daylight(Daylight::Partial), Outcome::Pass);
        assert!(is_fail(&check_daylight(Daylight::None)));
        assert!(is_unknown(&check_daylight(Daylight::Unknown)));
    }

    #[test]
    fn wifi_outcomes() {
        let request = EventRequest::partner_day(80);
        assert_eq!(check_wifi(Wifi::Good, &request), Outcome::Pass);
        assert_eq!(check_wifi(Wifi::Basic, &request), Outcome::Pass);
        assert!(is_fail(&check_wifi(Wifi::Poor, &request)));
        assert!(is_unknown(&check_wifi(Wifi::Unknown, &request)));
    }

    #[test]
    fn walk_outcomes() {
        let request = EventRequest::partner_day(80);
        let walk = |w| check_walk(w, "Utrecht Centraal", &request);
        assert_eq!(walk(WalkMinutes::Known(20)), Outcome::Pass);
        assert!(is_fail(&walk(WalkMinutes::Known(21))));
        assert!(is_unknown(&walk(WalkMinutes::Unknown)));
    }

    #[test]
    fn availability_outcomes() {
        let listed = |dates: &[u8], tentative| Availability::Listed {
            dates: dates.iter().copied().collect(),
            tentative,
        };
        assert_eq!(check_availability(&listed(&[9], false)), Outcome::Pass);
        assert!(is_fail(&check_availability(&listed(&[], false))));
        assert!(is_unknown(&check_availability(&listed(&[2, 9], true))));
        assert!(is_unknown(&check_availability(&listed(&[], true))));
        assert!(is_unknown(&check_availability(&Availability::Unknown)));
    }

    #[test]
    fn budget_outcomes() {
        let venues = real_venues();
        let request = EventRequest::partner_day(80);
        let cost = |euros| CostEstimate {
            venue_incl_vat: Money::from_euros(euros),
            catering: Money::from_cents(0),
            catering_assumed: false,
            all_in: Money::from_euros(euros),
        };
        let venue = &venues[0];
        assert_eq!(
            check_budget(Some(&cost(15_000)), venue, &request),
            Outcome::Pass
        );
        assert!(is_fail(&check_budget(Some(&cost(15_001)), venue, &request)));
        match check_budget(None, find_venue(&venues, "v029"), &request) {
            Outcome::Unknown(q) => assert!(q.contains("USD"), "{q}"),
            other => panic!("{other:?}"),
        }
    }

    fn find_venue<'a>(venues: &'a [Venue], id: &str) -> &'a Venue {
        venues.iter().find(|v| v.ids[0] == id).unwrap()
    }

    #[test]
    fn risks_name_the_assumptions() {
        let evaluations = shortlist(&real_venues(), &EventRequest::partner_day(80));
        let pakhuis = find(&evaluations, "v002");
        assert!(pakhuis.risks.iter().any(|r| r == "partial daylight only"));
        assert!(pakhuis.risks.iter().any(|r| r.starts_with("basic wifi")));
        let villa = find(&evaluations, "v003");
        assert!(villa
            .risks
            .iter()
            .any(|r| r.starts_with("catering only partly")));
        let fabriek = find(&evaluations, "v021");
        assert!(fabriek
            .risks
            .iter()
            .any(|r| r.starts_with("outside Utrecht")));
    }

    #[test]
    fn sorted_by_bucket_then_score_then_id() {
        let evaluations = shortlist(&real_venues(), &EventRequest::partner_day(80));
        for pair in evaluations.windows(2) {
            assert_ne!(rank_order(&pair[0], &pair[1]), Ordering::Greater);
        }
        assert_eq!(evaluations[0].bucket, Bucket::Shortlist);
        assert_eq!(evaluations.last().unwrap().bucket, Bucket::Excluded);
        // Excluded venues have no score, so they are ordered by id.
        let excluded: Vec<&str> = evaluations
            .iter()
            .filter(|e| e.bucket == Bucket::Excluded)
            .map(|e| e.venue.ids[0].as_str())
            .collect();
        let mut sorted = excluded.clone();
        sorted.sort();
        assert_eq!(excluded, sorted);
    }

    #[test]
    fn excluded_venues_are_not_scored() {
        let evaluations = shortlist(&real_venues(), &EventRequest::partner_day(80));
        for evaluation in &evaluations {
            assert_eq!(
                evaluation.score.is_some(),
                evaluation.bucket != Bucket::Excluded
            );
        }
    }
}
