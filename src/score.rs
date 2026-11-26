//! S: a transparent 100-point score that ranks venues within a bucket. Every weight is here so
//! the planner can see, and argue with, why one venue ranks above another.

use crate::evaluate::CostEstimate;
use crate::model::{Availability, BreakoutRooms, Daylight, Venue, WalkMinutes, Wifi};
use crate::request::{is_mid_march, EventDay, EventRequest};

pub const COST_MAX: f64 = 35.0;

/// The groups come from Amsterdam and Eindhoven, so Utrecht Centraal is the fair middle.
pub const LOCATION_CENTRAAL_SHORT_WALK: f64 = 25.0;
pub const LOCATION_CENTRAAL_SHORT_WALK_MINUTES: u32 = 10;
pub const LOCATION_CENTRAAL: f64 = 20.0;
pub const LOCATION_OTHER_UTRECHT: f64 = 15.0;
pub const LOCATION_OTHER_CITY: f64 = 8.0;
/// The home city of one group is far for the other, which defeats the point of meeting halfway.
pub const LOCATION_ORIGIN_CITY: f64 = 0.0;

pub const AVAILABILITY_PREFERRED_IN_WINDOW: f64 = 15.0;
pub const AVAILABILITY_OTHER_IN_WINDOW: f64 = 10.0;
pub const AVAILABILITY_OUTSIDE_WINDOW: f64 = 5.0;

pub const DAYLIGHT_YES: f64 = 10.0;
pub const DAYLIGHT_PARTIAL: f64 = 4.0;

pub const WIFI_GOOD: f64 = 10.0;
pub const WIFI_BASIC: f64 = 4.0;

pub const BREAKOUTS_FOUR_OR_MORE: f64 = 5.0;
pub const BREAKOUTS_THREE: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreBreakdown {
    pub cost: f64,
    pub location: f64,
    pub availability: f64,
    pub daylight: f64,
    pub wifi: f64,
    pub breakouts: f64,
}

impl ScoreBreakdown {
    pub fn total(&self) -> f64 {
        self.cost + self.location + self.availability + self.daylight + self.wifi + self.breakouts
    }
}

/// Where the venue is, judged from its nearest station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    UtrechtCentraal,
    OtherUtrecht,
    /// Amsterdam or Eindhoven, where the two groups come from.
    OriginCity,
    OtherCity,
    Unknown,
}

pub fn location(station: Option<&str>) -> Location {
    let Some(station) = station else {
        return Location::Unknown;
    };
    let station = station.trim().to_lowercase();
    if station == "utrecht centraal" || station == "utrecht cs" {
        Location::UtrechtCentraal
    } else if station.starts_with("utrecht") {
        Location::OtherUtrecht
    } else if station.starts_with("amsterdam") || station.starts_with("eindhoven") {
        Location::OriginCity
    } else {
        Location::OtherCity
    }
}

pub fn score(venue: &Venue, cost: Option<&CostEstimate>, request: &EventRequest) -> ScoreBreakdown {
    ScoreBreakdown {
        cost: cost_points(cost, request),
        location: location_points(venue),
        availability: availability_points(&venue.availability),
        daylight: match venue.daylight {
            Daylight::Yes => DAYLIGHT_YES,
            Daylight::Partial => DAYLIGHT_PARTIAL,
            Daylight::None | Daylight::Unknown => 0.0,
        },
        wifi: match venue.wifi {
            Wifi::Good => WIFI_GOOD,
            Wifi::Basic => WIFI_BASIC,
            Wifi::Poor | Wifi::Unknown => 0.0,
        },
        breakouts: match venue.breakout_rooms {
            BreakoutRooms::Known(n) if n >= 4 => BREAKOUTS_FOUR_OR_MORE,
            BreakoutRooms::Known(3) => BREAKOUTS_THREE,
            _ => 0.0,
        },
    }
}

/// Uses the all-in figure: even if catering has its own budget, the client pays it.
fn cost_points(cost: Option<&CostEstimate>, request: &EventRequest) -> f64 {
    let Some(cost) = cost else {
        return 0.0;
    };
    let share = cost.all_in.cents() as f64 / request.budget.cents() as f64;
    (COST_MAX * (1.0 - share)).clamp(0.0, COST_MAX)
}

/// An unknown walk from Utrecht Centraal scores as another Utrecht station: the venue is in
/// Utrecht, but an unconfirmed walk must not earn the walking bonus.
fn location_points(venue: &Venue) -> f64 {
    match location(venue.nearest_station.as_deref()) {
        Location::UtrechtCentraal => match venue.walk_minutes {
            WalkMinutes::Known(m) if m <= LOCATION_CENTRAAL_SHORT_WALK_MINUTES => {
                LOCATION_CENTRAAL_SHORT_WALK
            }
            WalkMinutes::Known(_) => LOCATION_CENTRAAL,
            WalkMinutes::Unknown => LOCATION_OTHER_UTRECHT,
        },
        Location::OtherUtrecht => LOCATION_OTHER_UTRECHT,
        Location::OtherCity => LOCATION_OTHER_CITY,
        Location::OriginCity | Location::Unknown => LOCATION_ORIGIN_CITY,
    }
}

/// Tentative dates still count: the bucket already says they need confirming.
fn availability_points(availability: &Availability) -> f64 {
    let Availability::Listed { dates, .. } = availability else {
        return 0.0;
    };
    let in_window = |day: EventDay| {
        dates
            .iter()
            .any(|d| is_mid_march(*d) && EventDay::of(*d) == Some(day))
    };
    // The client: "a Thursday would be ideal but Tuesday works too".
    if in_window(EventDay::Thursday) {
        AVAILABILITY_PREFERRED_IN_WINDOW
    } else if in_window(EventDay::Tuesday) {
        AVAILABILITY_OTHER_IN_WINDOW
    } else if !dates.is_empty() {
        AVAILABILITY_OUTSIDE_WINDOW
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Money;

    fn listed(days: &[u8]) -> Availability {
        Availability::Listed {
            dates: days.iter().copied().collect(),
            tentative: false,
        }
    }

    fn cost(all_in_euros: i64) -> CostEstimate {
        CostEstimate {
            venue_incl_vat: Money::from_euros(all_in_euros),
            catering: Money::from_euros(0),
            catering_assumed: false,
            all_in: Money::from_euros(all_in_euros),
        }
    }

    #[test]
    fn location_classes() {
        assert_eq!(
            location(Some("Utrecht Centraal")),
            Location::UtrechtCentraal
        );
        assert_eq!(location(Some("Utrecht CS")), Location::UtrechtCentraal);
        assert_eq!(
            location(Some("Utrecht Leidsche Rijn")),
            Location::OtherUtrecht
        );
        assert_eq!(
            location(Some("Amsterdam Muiderpoort")),
            Location::OriginCity
        );
        assert_eq!(location(Some("Eindhoven Strijp-S")), Location::OriginCity);
        assert_eq!(location(Some("'s-Hertogenbosch")), Location::OtherCity);
        assert_eq!(location(None), Location::Unknown);
    }

    #[test]
    fn cost_points_scale_and_clamp() {
        let request = EventRequest::partner_day(80);
        assert_eq!(cost_points(None, &request), 0.0);
        assert_eq!(cost_points(Some(&cost(0)), &request), 35.0);
        assert!((cost_points(Some(&cost(7_500)), &request) - 17.5).abs() < 1e-9);
        assert_eq!(cost_points(Some(&cost(20_000)), &request), 0.0);
    }

    #[test]
    fn availability_points_prefer_thursday_in_window() {
        assert_eq!(availability_points(&listed(&[11])), 15.0);
        assert_eq!(availability_points(&listed(&[9, 25])), 10.0);
        assert_eq!(availability_points(&listed(&[2, 25])), 5.0);
        assert_eq!(availability_points(&listed(&[])), 0.0);
        assert_eq!(availability_points(&Availability::Unknown), 0.0);
    }

    #[test]
    fn location_points_use_the_walk_from_centraal() {
        let venues = crate::load::parse_venues(include_str!("../venues.json"))
            .unwrap()
            .venues;
        let points = |id: &str| location_points(venues.iter().find(|v| v.ids[0] == id).unwrap());
        assert_eq!(points("v001"), 25.0); // 6 min from Centraal
        assert_eq!(points("v026"), 20.0); // 16 min from Centraal
        assert_eq!(points("v011"), 15.0); // Centraal, walk unknown
        assert_eq!(points("v006"), 15.0); // Utrecht Leidsche Rijn
        assert_eq!(points("v021"), 8.0); // 's-Hertogenbosch
        assert_eq!(points("v014"), 0.0); // Eindhoven
    }

    #[test]
    fn daylight_wifi_and_breakout_points() {
        let mut venue = crate::load::parse_venues(include_str!("../venues.json"))
            .unwrap()
            .venues
            .remove(0);
        let request = EventRequest::partner_day(80);
        let points = |venue: &Venue| {
            let s = score(venue, None, &request);
            (s.daylight, s.wifi, s.breakouts)
        };
        assert_eq!(points(&venue), (10.0, 10.0, 5.0));
        venue.daylight = Daylight::Partial;
        venue.wifi = Wifi::Basic;
        venue.breakout_rooms = BreakoutRooms::Known(3);
        assert_eq!(points(&venue), (4.0, 4.0, 3.0));
        venue.daylight = Daylight::Unknown;
        venue.wifi = Wifi::Unknown;
        venue.breakout_rooms = BreakoutRooms::Unknown;
        assert_eq!(points(&venue), (0.0, 0.0, 0.0));
    }

    #[test]
    fn total_adds_every_component() {
        let breakdown = ScoreBreakdown {
            cost: 10.5,
            location: 25.0,
            availability: 15.0,
            daylight: 4.0,
            wifi: 10.0,
            breakouts: 3.0,
        };
        assert!((breakdown.total() - 67.5).abs() < 1e-9);
    }
}
