//! Normalized venue data. Anything the source does not confirm is an explicit `Unknown`.

use std::collections::BTreeSet;
use std::fmt;

/// Dutch standard VAT rate, applied to venue prices quoted excl. VAT.
pub const VAT_RATE_PERCENT: i64 = 21;

/// An amount in euro cents. Integer cents keep the budget checks exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(i64);

impl Money {
    pub const fn from_cents(cents: i64) -> Self {
        Money(cents)
    }

    pub const fn from_euros(euros: i64) -> Self {
        Money(euros * 100)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    pub const fn times(self, factor: u32) -> Money {
        Money(self.0 * factor as i64)
    }

    /// Converts an excl.-VAT amount to incl. VAT, rounding half a cent up.
    pub fn add_vat(self) -> Money {
        Money((self.0 * (100 + VAT_RATE_PERCENT) + 50).div_euclid(100))
    }
}

impl std::ops::Add for Money {
    type Output = Money;

    fn add(self, other: Money) -> Money {
        Money(self.0 + other.0)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let euros = (abs / 100).to_string();
        let mut grouped = String::new();
        for (i, digit) in euros.chars().enumerate() {
            if i > 0 && (euros.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(digit);
        }
        write!(f, "{sign}€{grouped}.{:02}", abs % 100)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capacity {
    Known(u32),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakoutRooms {
    Known(u32),
    Unknown,
}

/// Walking time from `nearest_station` to the venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkMinutes {
    Known(u32),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accessibility {
    Yes,
    No,
    /// Qualified access ("ground floor only"): needs confirmation, so it is never a plain yes.
    Partial(String),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Daylight {
    Yes,
    Partial,
    None,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wifi {
    Good,
    Basic,
    Poor,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Catering {
    Included,
    NotIncluded,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceUnit {
    PerDay,
    PerPerson,
    /// Dutch "dagdeel" (morning or afternoon). A full day counts as 2 day-parts.
    PerDayPart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    pub amount: Money,
    pub unit: PriceUnit,
    /// The VAT basis of `amount` and `minimum_spend`.
    pub vat_included: bool,
    pub minimum_spend: Option<Money>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriceInfo {
    Known(Price),
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Unknown,
    /// `dates` are March 2027 days of the month, Tuesdays and Thursdays only.
    /// `tentative` means the listing is hedged ("most Thursdays") or does not name March.
    Listed {
        dates: BTreeSet<u8>,
        tentative: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Venue {
    /// Source record ids. One per record until duplicates are merged.
    pub ids: Vec<String>,
    pub name: String,
    pub city: Option<String>,
    pub nearest_station: Option<String>,
    pub walk_minutes: WalkMinutes,
    pub capacity: Capacity,
    pub breakout_rooms: BreakoutRooms,
    /// Every known price from the source records. Merged duplicates may quote different
    /// prices; the cost estimate uses the highest so the venue never looks cheaper than a source.
    /// Empty means the price is unknown.
    pub prices: Vec<Price>,
    /// Why no price is known. Only set when `prices` is empty.
    pub price_unknown_reason: Option<String>,
    pub catering: Catering,
    pub wheelchair: Accessibility,
    pub daylight: Daylight,
    pub wifi: Wifi,
    pub availability: Availability,
    /// The source notes plus anything the parsers kept from the raw text.
    pub notes: Vec<String>,
    /// Assumptions made while normalizing, to be shown to the planner.
    pub risks: Vec<String>,
    /// Fields where merged source records gave conflicting known values (D2, D4).
    pub disputed: Vec<Disputed>,
}

/// A conflict between two source records. The venue keeps the worse value; `field` carries the
/// most favourable source value so evaluation can tell when a fail is caused only by the
/// dispute (D4), in which case the honest answer is to ask the venue, not to exclude it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disputed {
    pub field: DisputedField,
    /// For the planner, e.g. "capacity: 120 vs 110".
    pub detail: String,
}

/// The disputed field, with the best value any source gave for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisputedField {
    Capacity {
        best: u32,
    },
    BreakoutRooms {
        best: u32,
    },
    WalkMinutes {
        best: u32,
    },
    Wheelchair {
        best: Accessibility,
    },
    Daylight {
        best: Daylight,
    },
    Wifi {
        best: Wifi,
    },
    /// The union of the listed dates, firm only if every source was firm.
    Availability {
        best: Availability,
    },
    Catering,
    /// Not a D4 trigger: the highest price is used for the budget check anyway.
    Price,
}

// The event is in March 2027 and March 1, 2027 is a Monday, so the weekday of a day in that
// month is a fixed offset. That is the only calendar this tool needs.

pub fn is_march_2027_tuesday(day: u8) -> bool {
    (1..=31).contains(&day) && (day - 1) % 7 == 1
}

pub fn is_march_2027_thursday(day: u8) -> bool {
    (1..=31).contains(&day) && (day - 1) % 7 == 3
}

pub fn is_march_2027_tue_or_thu(day: u8) -> bool {
    is_march_2027_tuesday(day) || is_march_2027_thursday(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn march_2027_tuesdays_and_thursdays() {
        let tue: Vec<u8> = (1..=31).filter(|d| is_march_2027_tuesday(*d)).collect();
        let thu: Vec<u8> = (1..=31).filter(|d| is_march_2027_thursday(*d)).collect();
        assert_eq!(tue, vec![2, 9, 16, 23, 30]);
        assert_eq!(thu, vec![4, 11, 18, 25]);
        assert!(!is_march_2027_tue_or_thu(0));
        assert!(!is_march_2027_tue_or_thu(32));
        assert!(!is_march_2027_tue_or_thu(1));
    }

    #[test]
    fn add_vat_is_21_percent_rounded_half_up() {
        assert_eq!(
            Money::from_euros(10_000).add_vat(),
            Money::from_euros(12_100)
        );
        // 0.50 * 1.21 = 0.605 -> 0.61
        assert_eq!(Money::from_cents(50).add_vat(), Money::from_cents(61));
        // 0.10 * 1.21 = 0.121 -> 0.12
        assert_eq!(Money::from_cents(10).add_vat(), Money::from_cents(12));
    }

    #[test]
    fn money_display_groups_thousands() {
        assert_eq!(Money::from_cents(1_210_050).to_string(), "€12,100.50");
        assert_eq!(Money::from_cents(7_250).to_string(), "€72.50");
        assert_eq!(Money::from_cents(5).to_string(), "€0.05");
    }
}
