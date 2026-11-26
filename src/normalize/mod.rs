//! Turns one raw record into a typed `Venue` (REQUIREMENTS N1–N8). Price and availability
//! have their own modules because their rules are the largest.

pub mod availability;
pub mod price;

use crate::model::{
    Accessibility, BreakoutRooms, Capacity, Catering, Daylight, Venue, WalkMinutes, Wifi,
};
use crate::raw::{RawCapacity, RawCount, RawFlag, RawVenue};

pub const LAYOUT_RISK: &str = "capacity depends on layout";
pub const RANGE_RISK: &str = "capacity listed as a range, upper bound used";

/// A venue needs an id and a name to be reported at all; everything else may be unknown.
pub fn normalize_venue(raw: RawVenue) -> Result<Venue, String> {
    let id = non_empty(raw.id).ok_or("missing id")?;
    let name = non_empty(raw.name).ok_or("missing name")?;

    let mut notes: Vec<String> = non_empty(raw.notes).into_iter().collect();
    let mut risks = Vec::new();

    let (capacity, capacity_risk) = parse_capacity(raw.capacity.as_ref());
    risks.extend(capacity_risk);
    let (breakout_rooms, breakout_note) = parse_breakout_rooms(raw.breakout_rooms.as_ref());
    notes.extend(breakout_note);
    let (price, price_risks) = price::parse_price(raw.price.as_ref());
    risks.extend(price_risks);

    Ok(Venue {
        ids: vec![id],
        name,
        city: non_empty(raw.city),
        nearest_station: non_empty(raw.nearest_station),
        walk_minutes: parse_walk_minutes(raw.walk_minutes.as_ref()),
        capacity,
        breakout_rooms,
        price,
        catering: parse_catering(raw.catering_included.as_ref()),
        wheelchair: parse_wheelchair(raw.wheelchair_accessible.as_ref()),
        daylight: parse_daylight(raw.daylight.as_deref()),
        wifi: parse_wifi(raw.wifi.as_deref()),
        availability: availability::parse_availability(raw.availability.as_deref()),
        notes,
        risks,
    })
}

/// N1. A zero is a broken export, not an empty room, so it is Unknown. A plenary is seated,
/// so "90 seated / 130 standing" is 90. Returns a risk when the number depends on the layout.
pub fn parse_capacity(raw: Option<&RawCapacity>) -> (Capacity, Option<String>) {
    let known = |n: Option<u32>| match n {
        Some(n) if n > 0 => Capacity::Known(n),
        _ => Capacity::Unknown,
    };
    match raw {
        None => (Capacity::Unknown, None),
        Some(RawCapacity::Number(n)) => (known(u32::try_from(*n).ok()), None),
        Some(RawCapacity::Text(text)) => {
            let capacity = known(capacity_from_text(text));
            // The top of a range is the venue's best case, so say so instead of hiding it.
            let risk = (capacity != Capacity::Unknown && parse_range(text.trim()).is_some())
                .then(|| format!("{RANGE_RISK} (\"{}\")", text.trim()));
            (capacity, risk)
        }
        Some(RawCapacity::Layouts(layouts)) => {
            let largest = layouts
                .values()
                .filter_map(|n| u32::try_from(*n).ok())
                .max();
            match known(largest) {
                Capacity::Unknown => (Capacity::Unknown, None),
                capacity => {
                    let listed: Vec<String> =
                        layouts.iter().map(|(k, v)| format!("{k} {v}")).collect();
                    let risk = format!("{LAYOUT_RISK} ({})", listed.join(", "));
                    (capacity, Some(risk))
                }
            }
        }
    }
}

fn capacity_from_text(text: &str) -> Option<u32> {
    let text = text.trim().to_lowercase();
    if let Some(at) = text.find("seated") {
        return last_number(&text[..at]);
    }
    if let Some((low, high)) = parse_range(&text) {
        return (low <= high).then_some(high);
    }
    text.parse().ok()
}

/// N2. A range gives the lower bound: fewer rooms is the safe reading. Trailing text
/// such as "(one is the lounge)" is kept as a note.
pub fn parse_breakout_rooms(raw: Option<&RawCount>) -> (BreakoutRooms, Option<String>) {
    match raw {
        None => (BreakoutRooms::Unknown, None),
        Some(RawCount::Number(n)) => match u32::try_from(*n) {
            Ok(n) => (BreakoutRooms::Known(n), None),
            Err(_) => (BreakoutRooms::Unknown, None),
        },
        Some(RawCount::Text(text)) => {
            let text = text.trim();
            if let Some((low, _)) = parse_range(text) {
                let note = format!("breakout rooms listed as \"{text}\", lower bound used");
                return (BreakoutRooms::Known(low), Some(note));
            }
            let Some((count, rest)) = leading_number(text) else {
                return (BreakoutRooms::Unknown, None);
            };
            let rest = rest.trim();
            let rest = rest
                .strip_prefix('(')
                .and_then(|r| r.strip_suffix(')'))
                .unwrap_or(rest)
                .trim();
            let note = (!rest.is_empty()).then(|| format!("breakout count note: {rest}"));
            (BreakoutRooms::Known(count), note)
        }
    }
}

/// N3. The first integer in the text ("~10 min" is 10).
pub fn parse_walk_minutes(raw: Option<&RawCount>) -> WalkMinutes {
    let minutes = match raw {
        None => None,
        Some(RawCount::Number(n)) => u32::try_from(*n).ok(),
        Some(RawCount::Text(text)) => first_number(text),
    };
    minutes.map_or(WalkMinutes::Unknown, WalkMinutes::Known)
}

/// N5.
pub fn parse_catering(raw: Option<&RawFlag>) -> Catering {
    match raw {
        Some(RawFlag::Bool(true)) => Catering::Included,
        Some(RawFlag::Bool(false)) => Catering::NotIncluded,
        Some(RawFlag::Text(text)) if text.trim().eq_ignore_ascii_case("partial") => {
            Catering::Partial
        }
        _ => Catering::Unknown,
    }
}

/// N6. Any qualified text ("ground floor only") is Partial: it needs confirmation.
pub fn parse_wheelchair(raw: Option<&RawFlag>) -> Accessibility {
    match raw {
        None => Accessibility::Unknown,
        Some(RawFlag::Bool(true)) => Accessibility::Yes,
        Some(RawFlag::Bool(false)) => Accessibility::No,
        Some(RawFlag::Text(text)) => {
            let text = text.trim();
            if text.is_empty() || text.eq_ignore_ascii_case("unknown") {
                Accessibility::Unknown
            } else {
                Accessibility::Partial(text.to_string())
            }
        }
    }
}

/// N7. The checks run in this order so "mixed — casemates have no windows" is Partial.
pub fn parse_daylight(raw: Option<&str>) -> Daylight {
    let Some(text) = raw else {
        return Daylight::Unknown;
    };
    let text = text.trim().to_lowercase();
    let words = words(&text);
    if ["partial", "limited", "mixed"]
        .iter()
        .any(|w| words.contains(w))
    {
        Daylight::Partial
    } else if words.first() == Some(&"no")
        || words.first() == Some(&"none")
        || text.contains("no windows")
        || text.contains("black box")
    {
        Daylight::None
    } else if text.starts_with("yes") {
        Daylight::Yes
    } else {
        Daylight::Unknown
    }
}

/// N8. Complaints win over good keywords, so the checks run Poor, Good, Basic.
pub fn parse_wifi(raw: Option<&str>) -> Wifi {
    let Some(text) = raw else {
        return Wifi::Unknown;
    };
    let text = text.to_lowercase();
    let mentions = |keywords: &[&str]| keywords.iter().any(|k| text.contains(k));
    if mentions(&["weak", "complaint", "4g", "poor", "bad", "terrible"]) {
        Wifi::Poor
    } else if mentions(&["fiber", "gbps", "mbps", "good", "dedicated"]) {
        Wifi::Good
    } else if mentions(&["guest", "fair", "basic"]) {
        Wifi::Basic
    } else {
        Wifi::Unknown
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Lowercase alphanumeric words; callers lowercase first.
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The number at the very start of `text`, and what follows it.
fn leading_number(text: &str) -> Option<(u32, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

fn first_number(text: &str) -> Option<u32> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    leading_number(&text[start..]).map(|(n, _)| n)
}

fn last_number(text: &str) -> Option<u32> {
    let end = text.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = text[..end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    text[start..end].parse().ok()
}

/// "80-120" or "80–120" (en dash), nothing else around it.
fn parse_range(text: &str) -> Option<(u32, u32)> {
    let (low, high) = text.split_once(['-', '–'])?;
    Some((low.trim().parse().ok()?, high.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::{Availability, PriceInfo};

    fn text(s: &str) -> RawCount {
        RawCount::Text(s.to_string())
    }

    #[test]
    fn capacity_positive_integer_is_used() {
        assert_eq!(
            parse_capacity(Some(&RawCapacity::Number(150))),
            (Capacity::Known(150), None)
        );
    }

    #[test]
    fn capacity_zero_null_or_negative_is_unknown() {
        assert_eq!(
            parse_capacity(Some(&RawCapacity::Number(0))).0,
            Capacity::Unknown
        );
        assert_eq!(
            parse_capacity(Some(&RawCapacity::Number(-5))).0,
            Capacity::Unknown
        );
        assert_eq!(parse_capacity(None).0, Capacity::Unknown);
    }

    #[test]
    fn capacity_range_uses_upper_bound() {
        let raw = RawCapacity::Text("80-120".into());
        let (capacity, risk) = parse_capacity(Some(&raw));
        assert_eq!(capacity, Capacity::Known(120));
        assert!(risk.unwrap().starts_with(RANGE_RISK));
    }

    #[test]
    fn capacity_seated_and_standing_uses_seated() {
        let raw = RawCapacity::Text("90 seated / 130 standing".into());
        assert_eq!(parse_capacity(Some(&raw)), (Capacity::Known(90), None));
    }

    #[test]
    fn capacity_layouts_use_largest_and_record_risk() {
        let layouts = BTreeMap::from([
            ("plenary_theatre".to_string(), 140),
            ("plenary_cabaret".to_string(), 90),
        ]);
        let (capacity, risk) = parse_capacity(Some(&RawCapacity::Layouts(layouts)));
        assert_eq!(capacity, Capacity::Known(140));
        assert!(risk.unwrap().starts_with(LAYOUT_RISK));
    }

    #[test]
    fn capacity_unparseable_text_is_unknown() {
        for s in ["up to 150", "large", "", "120-80"] {
            let raw = RawCapacity::Text(s.into());
            assert_eq!(parse_capacity(Some(&raw)).0, Capacity::Unknown, "{s}");
        }
    }

    #[test]
    fn breakout_integer_is_used() {
        assert_eq!(
            parse_breakout_rooms(Some(&RawCount::Number(4))),
            (BreakoutRooms::Known(4), None)
        );
    }

    #[test]
    fn breakout_range_uses_lower_bound() {
        let (rooms, note) = parse_breakout_rooms(Some(&text("4-6")));
        assert_eq!(rooms, BreakoutRooms::Known(4));
        assert!(note.is_some());
    }

    #[test]
    fn breakout_parenthetical_becomes_note() {
        assert_eq!(
            parse_breakout_rooms(Some(&text("3 (one is the lounge)"))),
            (
                BreakoutRooms::Known(3),
                Some("breakout count note: one is the lounge".to_string())
            )
        );
    }

    #[test]
    fn breakout_null_or_unparseable_is_unknown() {
        assert_eq!(parse_breakout_rooms(None).0, BreakoutRooms::Unknown);
        assert_eq!(
            parse_breakout_rooms(Some(&text("several"))).0,
            BreakoutRooms::Unknown
        );
    }

    #[test]
    fn walk_minutes_forms() {
        assert_eq!(
            parse_walk_minutes(Some(&RawCount::Number(6))),
            WalkMinutes::Known(6)
        );
        assert_eq!(
            parse_walk_minutes(Some(&text("~10 min"))),
            WalkMinutes::Known(10)
        );
        assert_eq!(parse_walk_minutes(None), WalkMinutes::Unknown);
        assert_eq!(
            parse_walk_minutes(Some(&text("close by"))),
            WalkMinutes::Unknown
        );
    }

    #[test]
    fn catering_forms() {
        assert_eq!(
            parse_catering(Some(&RawFlag::Bool(true))),
            Catering::Included
        );
        assert_eq!(
            parse_catering(Some(&RawFlag::Bool(false))),
            Catering::NotIncluded
        );
        assert_eq!(
            parse_catering(Some(&RawFlag::Text("partial".into()))),
            Catering::Partial
        );
        assert_eq!(parse_catering(None), Catering::Unknown);
    }

    #[test]
    fn wheelchair_forms() {
        assert_eq!(
            parse_wheelchair(Some(&RawFlag::Bool(true))),
            Accessibility::Yes
        );
        assert_eq!(
            parse_wheelchair(Some(&RawFlag::Bool(false))),
            Accessibility::No
        );
        assert_eq!(
            parse_wheelchair(Some(&RawFlag::Text("unknown".into()))),
            Accessibility::Unknown
        );
        assert_eq!(parse_wheelchair(None), Accessibility::Unknown);
        for s in [
            "ground floor only",
            "yes, except casemate rooms",
            "lift to 1st floor, breakout rooms on 2nd floor",
        ] {
            assert_eq!(
                parse_wheelchair(Some(&RawFlag::Text(s.into()))),
                Accessibility::Partial(s.into())
            );
        }
    }

    #[test]
    fn daylight_forms_from_dataset() {
        let cases = [
            ("yes", Daylight::Yes),
            ("yes, skylights", Daylight::Yes),
            ("yes, fully glazed", Daylight::Yes),
            ("yes, greenhouse", Daylight::Yes),
            ("partial", Daylight::Partial),
            ("limited", Daylight::Partial),
            ("mixed — casemates have no windows", Daylight::Partial),
            ("no windows in plenary", Daylight::None),
            ("no", Daylight::None),
            ("black box", Daylight::None),
            ("some", Daylight::Unknown),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_daylight(Some(input)), expected, "{input}");
        }
        assert_eq!(parse_daylight(None), Daylight::Unknown);
    }

    #[test]
    fn wifi_forms_from_dataset() {
        let cases = [
            ("fiber, 1 Gbps, dedicated event network", Wifi::Good),
            ("fiber 500 Mbps", Wifi::Good),
            ("fiber", Wifi::Good),
            ("good", Wifi::Good),
            ("guest wifi", Wifi::Basic),
            ("fair", Wifi::Basic),
            ("complaints reported", Wifi::Poor),
            ("4G backup only in casemates", Wifi::Poor),
            ("weak", Wifi::Poor),
            ("yes", Wifi::Unknown),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_wifi(Some(input)), expected, "{input}");
        }
        assert_eq!(parse_wifi(None), Wifi::Unknown);
    }

    #[test]
    fn venue_without_id_or_name_is_rejected() {
        let raw: RawVenue = serde_json::from_str(r#"{"name": "X"}"#).unwrap();
        assert!(normalize_venue(raw).is_err());
        let raw: RawVenue = serde_json::from_str(r#"{"id": "v1"}"#).unwrap();
        assert!(normalize_venue(raw).is_err());
    }

    #[test]
    fn venue_with_only_id_and_name_is_all_unknown() {
        let raw: RawVenue = serde_json::from_str(r#"{"id": "v1", "name": "X"}"#).unwrap();
        let venue = normalize_venue(raw).unwrap();
        assert_eq!(venue.ids, vec!["v1".to_string()]);
        assert_eq!(venue.city, None);
        assert_eq!(venue.capacity, Capacity::Unknown);
        assert_eq!(venue.walk_minutes, WalkMinutes::Unknown);
        assert!(matches!(venue.price, PriceInfo::Unknown { .. }));
        assert_eq!(venue.availability, Availability::Unknown);
        assert!(venue.notes.is_empty());
    }

    #[test]
    fn venue_collects_notes_and_risks() {
        let raw: RawVenue = serde_json::from_str(
            r#"{"id": "v2", "name": "Pakhuis", "breakout_rooms": "3 (one is the lounge)",
                "capacity": {"a": 140, "b": 90}, "price": "€4.200 per dagdeel",
                "notes": "Stairs only."}"#,
        )
        .unwrap();
        let venue = normalize_venue(raw).unwrap();
        assert_eq!(
            venue.notes,
            vec!["Stairs only.", "breakout count note: one is the lounge"]
        );
        assert_eq!(venue.risks.len(), 3, "{:?}", venue.risks);
    }
}
