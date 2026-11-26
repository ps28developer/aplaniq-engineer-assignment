//! The client's brief as data, so every rule in `evaluate` and `score` reads from one place.

use std::ops::RangeInclusive;

use crate::model::{is_march_2027_thursday, is_march_2027_tuesday, Money};

/// The client has not settled the headcount, so by default both ends of the range are run.
pub const HEADCOUNT_SCENARIOS: [u32; 2] = [80, 120];

/// The client asked for "mid-March": the two middle weeks of March 2027.
pub const MID_MARCH: RangeInclusive<u8> = 8..=19;

/// A Thursday lets people travel home before the weekend, so it scores highest.
pub const PREFERRED_DAY: EventDay = EventDay::Thursday;

pub const BUDGET: Money = Money::from_euros(15_000);

/// The dataset's own notes quote €35–40 pp for catering, so the top of that range is used.
pub const CATERING_ESTIMATE_PER_PERSON: Money = Money::from_euros(40);

/// The client asked for 3–4 breakout rooms; 3 is the minimum.
pub const MIN_BREAKOUT_ROOMS: u32 = 3;

/// "Easy by train": a longer walk from the station loses people, so it is a hard limit.
pub const MAX_WALK_MINUTES: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRequest {
    pub headcount: u32,
    /// For the venue cost incl. VAT. Catering may have its own budget, so it is not included.
    pub budget: Money,
    pub min_breakout_rooms: u32,
    pub max_walk_minutes: u32,
    /// Assumed, incl. VAT. Used only when the venue does not include catering.
    pub catering_estimate_per_person: Money,
}

impl EventRequest {
    pub fn partner_day(headcount: u32) -> Self {
        EventRequest {
            headcount,
            budget: BUDGET,
            min_breakout_rooms: MIN_BREAKOUT_ROOMS,
            max_walk_minutes: MAX_WALK_MINUTES,
            catering_estimate_per_person: CATERING_ESTIMATE_PER_PERSON,
        }
    }
}

/// The partner day is held on a Tuesday or a Thursday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDay {
    Tuesday,
    Thursday,
}

impl EventDay {
    /// The event day for a day of March 2027, or `None` when it is not a Tuesday or Thursday.
    pub fn of(day: u8) -> Option<EventDay> {
        if is_march_2027_tuesday(day) {
            Some(EventDay::Tuesday)
        } else if is_march_2027_thursday(day) {
            Some(EventDay::Thursday)
        } else {
            None
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            EventDay::Tuesday => "Tue",
            EventDay::Thursday => "Thu",
        }
    }
}

pub fn is_mid_march(day: u8) -> bool {
    MID_MARCH.contains(&day)
}

/// "Tue 9 Mar". Only March 2027 days are ever labelled.
pub fn day_label(day: u8) -> String {
    match EventDay::of(day) {
        Some(weekday) => format!("{} {day} Mar", weekday.short_name()),
        None => format!("{day} Mar"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partner_day_uses_the_brief() {
        let request = EventRequest::partner_day(80);
        assert_eq!(request.headcount, 80);
        assert_eq!(request.budget, Money::from_euros(15_000));
        assert_eq!(request.min_breakout_rooms, 3);
        assert_eq!(request.max_walk_minutes, 20);
        assert_eq!(request.catering_estimate_per_person, Money::from_euros(40));
    }

    #[test]
    fn day_labels_and_window() {
        assert_eq!(day_label(9), "Tue 9 Mar");
        assert_eq!(day_label(25), "Thu 25 Mar");
        assert_eq!(day_label(10), "10 Mar");
        let in_window: Vec<u8> = (1..=31)
            .filter(|d| EventDay::of(*d).is_some() && is_mid_march(*d))
            .collect();
        assert_eq!(in_window, vec![9, 11, 16, 18]);
    }
}
