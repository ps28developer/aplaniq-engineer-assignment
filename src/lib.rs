//! Venue shortlist: load the messy venue export, merge duplicates, check every venue against
//! the partner-day brief and rank what is left.

pub mod dedupe;
pub mod evaluate;
pub mod load;
pub mod model;
pub mod normalize;
pub mod raw;
pub mod report;
pub mod request;
pub mod score;
