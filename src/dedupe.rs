//! D1–D3: the export merges a partner sheet with scraped listings, so one venue can appear
//! twice under slightly different names. Duplicates are merged conservatively: the client
//! sees the worse of two conflicting values, and the conflict itself is kept.

use std::mem::discriminant;

use crate::model::{
    Accessibility, Availability, BreakoutRooms, Capacity, Catering, Daylight, Disputed,
    DisputedField, Price, PriceUnit, Venue, WalkMinutes, Wifi,
};

/// Words that do not tell two venues apart ("Het Spoorhuis" is "Spoorhuis").
const ARTICLES: [&str; 3] = ["het", "de", "the"];

/// What remains of a name once the decorations listings add are removed: a parenthetical
/// ("(Utrecht CS)"), a subtitle after a spaced dash ("— Event Floor"), articles and the city
/// itself ("Hotel Domzicht Utrecht").
pub fn name_key(name: &str, city: Option<&str>) -> String {
    let mut name = strip_parentheticals(&name.to_lowercase());
    for dash in [" — ", " – ", " - "] {
        if let Some(at) = name.find(dash) {
            name.truncate(at);
        }
    }
    let city = city.unwrap_or_default().to_lowercase();
    let city_words = words(&city);
    let key: String = words(&name)
        .into_iter()
        .filter(|w| !ARTICLES.contains(w) && !city_words.contains(w))
        .collect();
    if key.is_empty() {
        // A venue named only after its city ("Utrecht") still needs a key of its own.
        words(&name).concat()
    } else {
        key
    }
}

/// Merges records of the same venue, keeping the order of first occurrence.
pub fn merge_duplicates(venues: Vec<Venue>) -> Vec<Venue> {
    let mut merged: Vec<(String, Option<String>, Venue)> = Vec::new();
    for venue in venues {
        let key = name_key(&venue.name, venue.city.as_deref());
        let city = venue.city.as_ref().map(|c| c.trim().to_lowercase());
        match merged.iter_mut().find(|(k, c, _)| *k == key && *c == city) {
            Some((_, _, kept)) => merge_into(kept, venue),
            None => merged.push((key, city, venue)),
        }
    }
    merged.into_iter().map(|(_, _, venue)| venue).collect()
}

/// The first record's identity (name, city, station) is kept; every other field is merged.
fn merge_into(kept: &mut Venue, other: Venue) {
    kept.ids.extend(other.ids);
    for note in other.notes {
        if !kept.notes.contains(&note) {
            kept.notes.push(note);
        }
    }
    for risk in other.risks {
        if !kept.risks.contains(&risk) {
            kept.risks.push(risk);
        }
    }
    kept.disputed.extend(other.disputed);

    let capacity = merge_value(
        "capacity",
        kept.capacity,
        other.capacity,
        |c| matches!(c, Capacity::Known(_)),
        |c| match c {
            Capacity::Known(n) => i64::from(*n),
            Capacity::Unknown => 0,
        },
        |c| match c {
            Capacity::Known(n) => n.to_string(),
            Capacity::Unknown => "unknown".to_string(),
        },
    );
    kept.capacity = capacity.value;
    if let Some((Capacity::Known(best), detail)) = capacity.conflict {
        record(kept, DisputedField::Capacity { best }, detail);
    }

    let rooms = merge_value(
        "breakout rooms",
        kept.breakout_rooms,
        other.breakout_rooms,
        |r| matches!(r, BreakoutRooms::Known(_)),
        |r| match r {
            BreakoutRooms::Known(n) => i64::from(*n),
            BreakoutRooms::Unknown => 0,
        },
        |r| match r {
            BreakoutRooms::Known(n) => n.to_string(),
            BreakoutRooms::Unknown => "unknown".to_string(),
        },
    );
    kept.breakout_rooms = rooms.value;
    if let Some((BreakoutRooms::Known(best), detail)) = rooms.conflict {
        record(kept, DisputedField::BreakoutRooms { best }, detail);
    }

    // A longer walk is worse.
    let walk = merge_value(
        "walk minutes",
        kept.walk_minutes,
        other.walk_minutes,
        |w| matches!(w, WalkMinutes::Known(_)),
        |w| match w {
            WalkMinutes::Known(n) => -i64::from(*n),
            WalkMinutes::Unknown => 0,
        },
        |w| match w {
            WalkMinutes::Known(n) => n.to_string(),
            WalkMinutes::Unknown => "unknown".to_string(),
        },
    );
    kept.walk_minutes = walk.value;
    if let Some((WalkMinutes::Known(best), detail)) = walk.conflict {
        record(kept, DisputedField::WalkMinutes { best }, detail);
    }

    let wheelchair = merge_value(
        "wheelchair access",
        kept.wheelchair.clone(),
        other.wheelchair,
        |a| *a != Accessibility::Unknown,
        |a| match a {
            Accessibility::No | Accessibility::Unknown => 0,
            Accessibility::Partial(_) => 1,
            Accessibility::Yes => 2,
        },
        |a| match a {
            Accessibility::Yes => "yes".to_string(),
            Accessibility::No => "no".to_string(),
            Accessibility::Partial(text) => format!("\"{text}\""),
            Accessibility::Unknown => "unknown".to_string(),
        },
    );
    kept.wheelchair = wheelchair.value;
    if let Some((best, detail)) = wheelchair.conflict {
        record(kept, DisputedField::Wheelchair { best }, detail);
    }

    let daylight = merge_value(
        "daylight",
        kept.daylight,
        other.daylight,
        |d| *d != Daylight::Unknown,
        |d| match d {
            Daylight::None | Daylight::Unknown => 0,
            Daylight::Partial => 1,
            Daylight::Yes => 2,
        },
        |d| format!("{d:?}").to_lowercase(),
    );
    kept.daylight = daylight.value;
    if let Some((best, detail)) = daylight.conflict {
        record(kept, DisputedField::Daylight { best }, detail);
    }

    let wifi = merge_value(
        "wifi",
        kept.wifi,
        other.wifi,
        |w| *w != Wifi::Unknown,
        |w| match w {
            Wifi::Poor | Wifi::Unknown => 0,
            Wifi::Basic => 1,
            Wifi::Good => 2,
        },
        |w| format!("{w:?}").to_lowercase(),
    );
    kept.wifi = wifi.value;
    if let Some((best, detail)) = wifi.conflict {
        record(kept, DisputedField::Wifi { best }, detail);
    }

    // Catering not included is the worse case: it adds to the cost.
    let catering = merge_value(
        "catering",
        kept.catering,
        other.catering,
        |c| *c != Catering::Unknown,
        |c| match c {
            Catering::NotIncluded | Catering::Unknown => 0,
            Catering::Partial => 1,
            Catering::Included => 2,
        },
        |c| match c {
            Catering::Included => "included".to_string(),
            Catering::NotIncluded => "not included".to_string(),
            Catering::Partial => "partial".to_string(),
            Catering::Unknown => "unknown".to_string(),
        },
    );
    kept.catering = catering.value;
    if let Some((_, detail)) = catering.conflict {
        record(kept, DisputedField::Catering, detail);
    }

    merge_availability(kept, other.availability);
    merge_prices(kept, other.prices, other.price_unknown_reason);
}

struct Merged<T> {
    value: T,
    /// The better of the two values and the human detail, when both were known and differ.
    conflict: Option<(T, String)>,
}

/// A known value beats an unknown one; of two different known values the lower `rank` is kept.
fn merge_value<T: Clone + PartialEq>(
    label: &str,
    kept: T,
    other: T,
    known: impl Fn(&T) -> bool,
    rank: impl Fn(&T) -> i64,
    show: impl Fn(&T) -> String,
) -> Merged<T> {
    match (known(&kept), known(&other)) {
        (_, false) => Merged {
            value: kept,
            conflict: None,
        },
        (false, true) => Merged {
            value: other,
            conflict: None,
        },
        (true, true) if kept == other => Merged {
            value: kept,
            conflict: None,
        },
        (true, true) => {
            let detail = format!("{label}: {} vs {}", show(&kept), show(&other));
            let (worse, better) = if rank(&other) < rank(&kept) {
                (other, kept)
            } else {
                (kept, other)
            };
            Merged {
                value: worse,
                conflict: Some((better, detail)),
            }
        }
    }
}

/// Both sources must agree a date is free, so the dates are intersected. The union is kept as
/// the best case for D4.
fn merge_availability(kept: &mut Venue, other: Availability) {
    let Availability::Listed {
        dates: other_dates,
        tentative: other_tentative,
    } = other
    else {
        return;
    };
    let Availability::Listed { dates, tentative } = &kept.availability else {
        kept.availability = Availability::Listed {
            dates: other_dates,
            tentative: other_tentative,
        };
        return;
    };
    if *dates == other_dates && *tentative == other_tentative {
        return;
    }
    let show = |dates: &std::collections::BTreeSet<u8>, tentative: bool| {
        let days: Vec<String> = dates.iter().map(u8::to_string).collect();
        let hedge = if tentative { " (tentative)" } else { "" };
        format!("{{{}}}{hedge}", days.join(", "))
    };
    let detail = format!(
        "availability: {} vs {}",
        show(dates, *tentative),
        show(&other_dates, other_tentative)
    );
    let best = Availability::Listed {
        dates: dates.union(&other_dates).copied().collect(),
        tentative: *tentative && other_tentative,
    };
    let merged = Availability::Listed {
        dates: dates.intersection(&other_dates).copied().collect(),
        tentative: *tentative || other_tentative,
    };
    kept.availability = merged;
    record(kept, DisputedField::Availability { best }, detail);
}

/// All known prices are kept; the cost estimate takes the highest.
fn merge_prices(kept: &mut Venue, other: Vec<Price>, other_reason: Option<String>) {
    if other.is_empty() {
        if kept.prices.is_empty() && kept.price_unknown_reason.is_none() {
            kept.price_unknown_reason = other_reason;
        }
        return;
    }
    if kept.prices.is_empty() {
        kept.prices = other;
        kept.price_unknown_reason = None;
        return;
    }
    let differs = other.iter().any(|p| !kept.prices.contains(p));
    if differs {
        let show = |prices: &[Price]| {
            prices
                .iter()
                .map(describe_price)
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let detail = format!("price: {} vs {}", show(&kept.prices), show(&other));
        for price in other {
            if !kept.prices.contains(&price) {
                kept.prices.push(price);
            }
        }
        record(kept, DisputedField::Price, detail);
    }
}

pub fn describe_price(price: &Price) -> String {
    let unit = match price.unit {
        PriceUnit::PerDay => "per day",
        PriceUnit::PerPerson => "pp",
        PriceUnit::PerDayPart => "per day-part",
    };
    let vat = if price.vat_included { "incl." } else { "excl." };
    let mut text = format!("{} {unit} {vat} VAT", price.amount);
    if let Some(minimum) = price.minimum_spend {
        text.push_str(&format!(", minimum spend {minimum}"));
    }
    text
}

/// One entry per field: a third conflicting record widens the existing dispute.
fn record(venue: &mut Venue, field: DisputedField, detail: String) {
    match venue
        .disputed
        .iter_mut()
        .find(|d| discriminant(&d.field) == discriminant(&field))
    {
        Some(existing) => {
            existing.detail = format!("{}; {detail}", existing.detail);
            existing.field = better_case(&existing.field, field);
        }
        None => venue.disputed.push(Disputed { field, detail }),
    }
}

fn better_case(a: &DisputedField, b: DisputedField) -> DisputedField {
    use DisputedField as F;
    match (a, b) {
        (F::Capacity { best: x }, F::Capacity { best: y }) => F::Capacity { best: y.max(*x) },
        (F::BreakoutRooms { best: x }, F::BreakoutRooms { best: y }) => {
            F::BreakoutRooms { best: y.max(*x) }
        }
        (F::WalkMinutes { best: x }, F::WalkMinutes { best: y }) => {
            F::WalkMinutes { best: y.min(*x) }
        }
        // For the enum fields any not-failing best value is enough for D4; keep the newest.
        (_, b) => b,
    }
}

fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

fn strip_parentheticals(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::parse_venues;
    use crate::model::Money;

    fn real_venues() -> Vec<Venue> {
        parse_venues(include_str!("../venues.json")).unwrap().venues
    }

    fn by_first_id<'a>(venues: &'a [Venue], id: &str) -> &'a Venue {
        venues
            .iter()
            .find(|v| v.ids[0] == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    }

    fn venue(id: &str, name: &str) -> Venue {
        Venue {
            ids: vec![id.to_string()],
            name: name.to_string(),
            city: Some("Utrecht".to_string()),
            nearest_station: Some("Utrecht Centraal".to_string()),
            walk_minutes: WalkMinutes::Unknown,
            capacity: Capacity::Unknown,
            breakout_rooms: BreakoutRooms::Unknown,
            prices: Vec::new(),
            price_unknown_reason: Some("price not stated".to_string()),
            catering: Catering::Unknown,
            wheelchair: Accessibility::Unknown,
            daylight: Daylight::Unknown,
            wifi: Wifi::Unknown,
            availability: Availability::Unknown,
            notes: Vec::new(),
            risks: Vec::new(),
            disputed: Vec::new(),
        }
    }

    #[test]
    fn name_keys_of_the_real_pairs_match() {
        let utrecht = Some("Utrecht");
        assert_eq!(name_key("Spoorhuis Utrecht", utrecht), "spoorhuis");
        assert_eq!(name_key("Het Spoorhuis (Utrecht CS)", utrecht), "spoorhuis");
        assert_eq!(name_key("Hotel Domzicht", utrecht), "hoteldomzicht");
        assert_eq!(name_key("Hotel Domzicht Utrecht", utrecht), "hoteldomzicht");
        assert_eq!(name_key("Hub Moreelse", utrecht), "hubmoreelse");
        assert_eq!(
            name_key("HUB Moreelse — Event Floor", utrecht),
            "hubmoreelse"
        );
        assert_eq!(name_key("Utrecht", utrecht), "utrecht");
    }

    #[test]
    fn the_three_real_duplicate_pairs_merge() {
        let merged = merge_duplicates(real_venues());
        assert_eq!(merged.len(), 27);
        let ids: Vec<Vec<String>> = merged
            .iter()
            .filter(|v| v.ids.len() > 1)
            .map(|v| v.ids.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                vec!["v004".to_string(), "v005".to_string()],
                vec!["v009".to_string(), "v010".to_string()],
                vec!["v023".to_string(), "v024".to_string()],
            ]
        );
        // The first record's identity is kept, and the input order is preserved.
        assert_eq!(merged[3].name, "Spoorhuis Utrecht");
        assert_eq!(merged[0].ids, vec!["v001".to_string()]);
    }

    #[test]
    fn distinct_venues_do_not_merge() {
        let merged = merge_duplicates(vec![
            venue("v001", "De Seinwachter"),
            venue("v019", "De Stadstuin"),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn same_name_in_another_city_does_not_merge() {
        let mut other = venue("b", "Spoorhuis");
        other.city = Some("Amersfoort".to_string());
        let merged = merge_duplicates(vec![venue("a", "Spoorhuis"), other]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn known_beats_unknown_without_a_dispute() {
        let mut a = venue("a", "X");
        a.capacity = Capacity::Known(100);
        let mut b = venue("b", "X");
        b.wifi = Wifi::Good;
        b.availability = Availability::Listed {
            dates: [9, 11].into(),
            tentative: false,
        };
        b.prices = vec![Price {
            amount: Money::from_euros(5000),
            unit: PriceUnit::PerDay,
            vat_included: true,
            minimum_spend: None,
        }];
        b.price_unknown_reason = None;
        let merged = merge_duplicates(vec![a, b]).remove(0);
        assert_eq!(merged.capacity, Capacity::Known(100));
        assert_eq!(merged.wifi, Wifi::Good);
        assert_eq!(merged.prices.len(), 1);
        assert_eq!(merged.price_unknown_reason, None);
        assert!(matches!(merged.availability, Availability::Listed { .. }));
        assert!(merged.disputed.is_empty(), "{:?}", merged.disputed);
    }

    #[test]
    fn conflicting_known_values_keep_the_worse_and_are_disputed() {
        let mut a = venue("a", "X");
        a.capacity = Capacity::Known(120);
        a.walk_minutes = WalkMinutes::Known(9);
        a.daylight = Daylight::Yes;
        a.availability = Availability::Listed {
            dates: [9, 11, 16].into(),
            tentative: false,
        };
        let mut b = venue("b", "X");
        b.capacity = Capacity::Known(110);
        b.walk_minutes = WalkMinutes::Known(10);
        b.daylight = Daylight::Partial;
        b.availability = Availability::Listed {
            dates: [11, 16, 18].into(),
            tentative: true,
        };
        let merged = merge_duplicates(vec![a, b]).remove(0);
        assert_eq!(merged.capacity, Capacity::Known(110));
        assert_eq!(merged.walk_minutes, WalkMinutes::Known(10));
        assert_eq!(merged.daylight, Daylight::Partial);
        assert_eq!(
            merged.availability,
            Availability::Listed {
                dates: [11, 16].into(),
                tentative: true
            }
        );
        let fields: Vec<&DisputedField> = merged.disputed.iter().map(|d| &d.field).collect();
        assert_eq!(
            fields,
            vec![
                &DisputedField::Capacity { best: 120 },
                &DisputedField::WalkMinutes { best: 9 },
                &DisputedField::Daylight {
                    best: Daylight::Yes
                },
                &DisputedField::Availability {
                    best: Availability::Listed {
                        dates: [9, 11, 16, 18].into(),
                        tentative: false
                    }
                },
            ]
        );
        assert_eq!(merged.disputed[0].detail, "capacity: 120 vs 110");
    }

    #[test]
    fn domzicht_capacity_and_prices() {
        let merged = merge_duplicates(real_venues());
        let domzicht = by_first_id(&merged, "v009");
        assert_eq!(domzicht.capacity, Capacity::Known(110));
        assert_eq!(domzicht.prices.len(), 2);
        assert_eq!(
            domzicht.availability,
            Availability::Listed {
                dates: [9, 11].into(),
                tentative: false
            }
        );
        assert!(domzicht
            .disputed
            .iter()
            .any(|d| d.detail == "capacity: 120 vs 110"));
    }

    #[test]
    fn hub_moreelse_keeps_both_prices_as_a_dispute() {
        let merged = merge_duplicates(real_venues());
        let hub = by_first_id(&merged, "v023");
        let amounts: Vec<Money> = hub.prices.iter().map(|p| p.amount).collect();
        assert_eq!(
            amounts,
            vec![Money::from_euros(8900), Money::from_euros(9400)]
        );
        assert!(hub.disputed.iter().any(|d| d.field == DisputedField::Price));
    }

    #[test]
    fn spoorhuis_merge_keeps_the_known_bad_values() {
        let merged = merge_duplicates(real_venues());
        let spoorhuis = by_first_id(&merged, "v004");
        assert_eq!(spoorhuis.daylight, Daylight::None);
        assert_eq!(spoorhuis.wifi, Wifi::Poor);
        assert_eq!(spoorhuis.capacity, Capacity::Known(140));
        // v005 only adds unknowns for daylight and wifi, so they are not disputed.
        assert!(!spoorhuis.disputed.iter().any(|d| matches!(
            d.field,
            DisputedField::Daylight { .. } | DisputedField::Wifi { .. }
        )));
    }
}
