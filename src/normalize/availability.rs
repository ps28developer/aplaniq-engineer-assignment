//! N9: free-text availability into the set of March 2027 Tuesdays/Thursdays it lists.

use std::collections::BTreeSet;

use super::words;
use crate::model::{
    is_march_2027_thursday, is_march_2027_tue_or_thu, is_march_2027_tuesday, Availability,
};

pub fn parse_availability(raw: Option<&str>) -> Availability {
    let Some(text) = raw else {
        return Availability::Unknown;
    };
    let text = text.trim().to_lowercase();
    let all_words = words(&text);
    if all_words.is_empty() || all_words.iter().any(|w| matches!(*w, "ask" | "unknown")) {
        return Availability::Unknown;
    }

    // "Tuesdays" without a month may describe the venue's usual week, not March 2027.
    // Q1 contains March, so "fully booked Q1 2027" is a firm statement about March.
    let mentions_march = all_words
        .iter()
        .any(|w| matches!(*w, "march" | "maart" | "q1"));
    let tentative = all_words.contains(&"most") || !mentions_march;

    let (listed, exception) = match text.split_once("except") {
        Some((before, after)) => (before, Some(after)),
        None => (text.as_str(), None),
    };

    let mut dates = if text.contains("fully booked") {
        // Here the exception is the only thing that is available.
        exception.map(day_numbers).unwrap_or_default()
    } else {
        let explicit = day_numbers(listed);
        let mut dates = if explicit.is_empty() {
            match weekday_dates(listed) {
                Some(dates) => dates,
                None => return Availability::Unknown,
            }
        } else {
            explicit
        };
        for day in exception.map(day_numbers).unwrap_or_default() {
            dates.remove(&day);
        }
        dates
    };
    dates.retain(|day| is_march_2027_tue_or_thu(*day));
    Availability::Listed { dates, tentative }
}

/// Day-of-month numbers in the text. "book 6 weeks ahead" is a lead time, not a date.
fn day_numbers(text: &str) -> BTreeSet<u8> {
    let words = words(text);
    words
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            !words
                .get(i + 1)
                .is_some_and(|next| next.starts_with("week"))
        })
        .filter_map(|(_, word)| word.parse::<u8>().ok())
        .filter(|day| (1..=31).contains(day))
        .collect()
}

/// The March 2027 dates selected by weekday words, or `None` if the text names no weekday.
fn weekday_dates(text: &str) -> Option<BTreeSet<u8>> {
    let words = words(text);
    let all_weekdays = text.contains("mon-fri") || words.iter().any(|w| w.starts_with("weekday"));
    let tuesday = all_weekdays || words.iter().any(|w| w.starts_with("tue"));
    let thursday = all_weekdays || words.iter().any(|w| w.starts_with("thu"));
    if !tuesday && !thursday {
        return None;
    }
    let dates = (1..=31)
        .filter(|day| {
            (tuesday && is_march_2027_tuesday(*day)) || (thursday && is_march_2027_thursday(*day))
        })
        .collect();
    Some(dates)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUE: [u8; 5] = [2, 9, 16, 23, 30];
    const THU: [u8; 4] = [4, 11, 18, 25];

    fn listed(days: &[u8], tentative: bool) -> Availability {
        Availability::Listed {
            dates: days.iter().copied().collect(),
            tentative,
        }
    }

    fn tue_and_thu() -> Vec<u8> {
        TUE.iter().chain(THU.iter()).copied().collect()
    }

    #[test]
    fn unknown_forms() {
        for text in ["ask", "unknown", "", "   "] {
            assert_eq!(
                parse_availability(Some(text)),
                Availability::Unknown,
                "{text}"
            );
        }
        assert_eq!(parse_availability(None), Availability::Unknown);
        assert_eq!(
            parse_availability(Some("available in March")),
            Availability::Unknown
        );
    }

    #[test]
    fn every_listed_form_in_the_dataset() {
        let all = tue_and_thu();
        let cases: Vec<(&str, Availability)> = vec![
            ("Tue and Thu available in March 2027", listed(&all, false)),
            (
                "March: Thursdays only, except 18 March",
                listed(&[4, 11, 25], false),
            ),
            ("Tue/Thu March 2027 open", listed(&all, false)),
            ("Mon-Fri in March, book 6 weeks ahead", listed(&all, false)),
            ("Thursdays in March", listed(&THU, false)),
            ("Tuesdays only", listed(&TUE, true)),
            (
                "March 2027: fully booked except 9 and 11 March",
                listed(&[9, 11], false),
            ),
            ("Tue/Thu in March", listed(&all, false)),
            ("most Thursdays in March", listed(&THU, true)),
            ("weekdays in March", listed(&all, false)),
            ("fully booked Q1 2027", listed(&[], false)),
            ("Tue and Thu in March", listed(&all, false)),
            ("Tuesdays", listed(&TUE, true)),
            (
                "Tue/Thu in March, Tuesdays preferred by venue",
                listed(&all, false),
            ),
            ("Tue/Thu", listed(&all, true)),
            (
                "Tue 9, 16, 23 March; Thu 11, 25 March",
                listed(&[9, 11, 16, 23, 25], false),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_availability(Some(text)), expected, "{text}");
        }
    }

    #[test]
    fn fully_booked_except_filters_to_tue_thu() {
        // 10 March 2027 is a Wednesday.
        assert_eq!(
            parse_availability(Some("fully booked in March except 9 and 10 March")),
            listed(&[9], false)
        );
    }

    #[test]
    fn explicit_non_tue_thu_dates_are_dropped() {
        assert_eq!(
            parse_availability(Some("1, 2 and 3 March")),
            listed(&[2], false)
        );
    }
}
