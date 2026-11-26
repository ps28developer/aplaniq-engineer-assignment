//! O1–O4: the plain-text report a planner reads in a terminal. Formatting only: every
//! decision shown here was made in `evaluate` and `score`.

use std::fmt::Write;

use crate::evaluate::{Bucket, Evaluation};
use crate::model::{Availability, Venue};
use crate::request::{day_label, is_mid_march, EventRequest, MID_MARCH, PREFERRED_DAY};
use crate::score::ScoreBreakdown;

pub struct Scenario {
    pub request: EventRequest,
    /// Ranked, as returned by `evaluate::shortlist`.
    pub evaluations: Vec<Evaluation>,
}

/// `works_for_all` is printed only when more than one scenario ran.
pub fn render(scenarios: &[Scenario], works_for_all: &[&Venue]) -> String {
    let mut out = String::new();
    for scenario in scenarios {
        render_scenario(&mut out, scenario);
    }
    if scenarios.len() > 1 {
        let headcounts: Vec<String> = scenarios
            .iter()
            .map(|s| s.request.headcount.to_string())
            .collect();
        let _ = writeln!(
            out,
            "Works for both {} (shortlisted in every scenario):",
            headcounts.join(" and ")
        );
        if works_for_all.is_empty() {
            let _ = writeln!(out, "  (none)");
        }
        for venue in works_for_all {
            let _ = writeln!(out, "  - {}", title(venue));
        }
    }
    out
}

fn render_scenario(out: &mut String, scenario: &Scenario) {
    let request = &scenario.request;
    let rule = "=".repeat(72);
    let _ = writeln!(out, "{rule}");
    let _ = writeln!(out, "Partner day, March 2027: {} people", request.headcount);
    let _ = writeln!(out, "{rule}");
    let _ = writeln!(out, "Assumptions:");
    let _ = writeln!(
        out,
        "  - budget {} for the venue incl. VAT; an all-in total above it is a risk, not a fail",
        request.budget
    );
    let _ = writeln!(
        out,
        "  - catering estimate {} pp incl. VAT (assumed) when the venue does not include it",
        request.catering_estimate_per_person
    );
    let _ = writeln!(
        out,
        "  - dates: Tuesdays and Thursdays in March 2027; mid-March = {}-{} March (marked *), {} preferred",
        MID_MARCH.start(),
        MID_MARCH.end(),
        PREFERRED_DAY.short_name()
    );
    let _ = writeln!(
        out,
        "  - hard limits: at least {} breakout rooms, at most {} min walk from the station",
        request.min_breakout_rooms, request.max_walk_minutes
    );
    let _ = writeln!(out);

    let in_bucket = |bucket: Bucket| {
        scenario
            .evaluations
            .iter()
            .filter(move |e| e.bucket == bucket)
    };

    let shortlist: Vec<&Evaluation> = in_bucket(Bucket::Shortlist).collect();
    let _ = writeln!(out, "SHORTLIST ({})", shortlist.len());
    for (i, evaluation) in shortlist.iter().enumerate() {
        render_venue(out, i + 1, evaluation);
    }
    let _ = writeln!(out);

    let pending: Vec<&Evaluation> = in_bucket(Bucket::NeedsConfirmation).collect();
    let _ = writeln!(out, "NEEDS CONFIRMATION ({})", pending.len());
    for (i, evaluation) in pending.iter().enumerate() {
        render_venue(out, i + 1, evaluation);
        let _ = writeln!(out, "    Ask the venue:");
        for question in evaluation.questions() {
            let _ = writeln!(out, "      ? {question}");
        }
    }
    let _ = writeln!(out);

    let excluded: Vec<&Evaluation> = in_bucket(Bucket::Excluded).collect();
    let _ = writeln!(out, "EXCLUDED ({})", excluded.len());
    for evaluation in excluded {
        let reasons: Vec<String> = evaluation
            .fail_reasons()
            .map(|(requirement, reason)| format!("{requirement}: {reason}"))
            .collect();
        let _ = writeln!(
            out,
            "  - {}: {}",
            title(&evaluation.venue),
            reasons.join("; ")
        );
    }
    let _ = writeln!(out);
}

fn render_venue(out: &mut String, rank: usize, evaluation: &Evaluation) {
    let venue = &evaluation.venue;
    let _ = writeln!(out, "{rank:>2}. {}", title(venue));
    if let Some(score) = &evaluation.score {
        let _ = writeln!(out, "    score  {}", score_line(score));
    }
    let cost_line = match &evaluation.cost {
        Some(cost) => {
            let catering = if cost.catering_assumed {
                format!("catering {} assumed", cost.catering)
            } else {
                "catering included".to_string()
            };
            format!(
                "venue {} incl. VAT; all-in {} ({catering})",
                cost.venue_incl_vat, cost.all_in
            )
        }
        None => format!(
            "unknown ({})",
            venue
                .price_unknown_reason
                .as_deref()
                .unwrap_or("price not stated")
        ),
    };
    let _ = writeln!(out, "    cost   {cost_line}");
    let _ = writeln!(out, "    dates  {}", dates_line(&venue.availability));
    if !evaluation.risks.is_empty() {
        let _ = writeln!(out, "    risks");
        for risk in &evaluation.risks {
            let _ = writeln!(out, "      - {risk}");
        }
    }
    if !venue.notes.is_empty() {
        let _ = writeln!(out, "    notes  {}", venue.notes.join(" | "));
    }
}

fn title(venue: &Venue) -> String {
    format!("{} [{}]", venue.name, venue.ids.join(", "))
}

fn score_line(score: &ScoreBreakdown) -> String {
    format!(
        "{:.1}/100 = cost {:.1} + location {:.1} + availability {:.1} + daylight {:.1} + wifi {:.1} + breakouts {:.1}",
        score.total(),
        score.cost,
        score.location,
        score.availability,
        score.daylight,
        score.wifi,
        score.breakouts
    )
}

fn dates_line(availability: &Availability) -> String {
    match availability {
        Availability::Unknown => "unknown".to_string(),
        Availability::Listed { dates, .. } if dates.is_empty() => "none".to_string(),
        Availability::Listed { dates, tentative } => {
            let days: Vec<String> = dates
                .iter()
                .map(|d| {
                    let mark = if is_mid_march(*d) { "*" } else { "" };
                    format!("{}{mark}", day_label(*d))
                })
                .collect();
            let hedge = if *tentative { " (tentative)" } else { "" };
            format!("{}{hedge}", days.join(", "))
        }
    }
}
