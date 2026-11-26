//! The tech lead's hand-computed buckets (.planning/phases/02-evaluate/EXPECTED.md) against
//! the real dataset.

use std::path::Path;

use venue_shortlist::evaluate::{shortlist, shortlisted_in_all, Bucket, Evaluation};
use venue_shortlist::load::load_venues;
use venue_shortlist::request::EventRequest;

use Bucket::{Excluded as X, NeedsConfirmation as NC, Shortlist as S};

/// (ids, bucket at 80, bucket at 120), copied from EXPECTED.md.
const EXPECTED: [(&[&str], Bucket, Bucket); 27] = [
    (&["v001"], S, S),
    (&["v002"], NC, NC),
    (&["v003"], X, X),
    (&["v004", "v005"], X, X),
    (&["v006"], S, S),
    (&["v007"], X, X),
    (&["v008"], NC, X),
    (&["v009", "v010"], S, NC),
    (&["v011"], NC, NC),
    (&["v012"], NC, NC),
    (&["v013"], X, X),
    (&["v014"], S, S),
    (&["v015"], NC, NC),
    (&["v016"], X, X),
    (&["v017"], NC, X),
    (&["v018"], X, X),
    (&["v019"], S, X),
    (&["v020"], X, X),
    (&["v021"], S, S),
    (&["v022"], X, X),
    (&["v023", "v024"], S, S),
    (&["v025"], X, X),
    (&["v026"], S, S),
    (&["v027"], NC, NC),
    (&["v029"], NC, NC),
    (&["v028"], S, X),
    (&["v030"], S, S),
];

const WORKS_FOR_BOTH: [&str; 7] = ["v001", "v006", "v014", "v021", "v023", "v026", "v030"];

fn run(headcount: u32) -> Vec<Evaluation> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("venues.json");
    let loaded = load_venues(&path).expect("venues.json loads");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    shortlist(&loaded.venues, &EventRequest::partner_day(headcount))
}

fn bucket_of(evaluations: &[Evaluation], ids: &[&str]) -> Bucket {
    evaluations
        .iter()
        .find(|e| e.venue.ids == ids)
        .unwrap_or_else(|| panic!("no evaluation with ids {ids:?}"))
        .bucket
}

#[test]
fn every_venue_lands_in_the_expected_bucket() {
    let at_80 = run(80);
    let at_120 = run(120);
    assert_eq!(at_80.len(), EXPECTED.len());
    assert_eq!(at_120.len(), EXPECTED.len());
    let mut mismatches = Vec::new();
    for (ids, expected_80, expected_120) in EXPECTED {
        let actual_80 = bucket_of(&at_80, ids);
        let actual_120 = bucket_of(&at_120, ids);
        if actual_80 != expected_80 {
            mismatches.push(format!(
                "{ids:?} at 80: {actual_80:?}, expected {expected_80:?}"
            ));
        }
        if actual_120 != expected_120 {
            mismatches.push(format!(
                "{ids:?} at 120: {actual_120:?}, expected {expected_120:?}"
            ));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn works_for_both_80_and_120() {
    let scenarios = [run(80), run(120)];
    let mut ids: Vec<&str> = shortlisted_in_all(&scenarios)
        .iter()
        .map(|v| v.ids[0].as_str())
        .collect();
    ids.sort();
    assert_eq!(ids, WORKS_FOR_BOTH);
}
