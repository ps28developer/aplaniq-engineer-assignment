//! Serde types that mirror `venues.json` exactly, messy shapes included.
//!
//! Every field is optional: a missing field is an unknown, not an error. Interpretation of the
//! values happens in `normalize`, never here.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Venues stay as raw JSON values so each record can be deserialized on its own and one
/// malformed record cannot abort the whole run (N10).
#[derive(Debug, Deserialize)]
pub struct RawFile {
    pub venues: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawVenue {
    pub id: Option<String>,
    pub name: Option<String>,
    pub city: Option<String>,
    pub nearest_station: Option<String>,
    pub walk_minutes: Option<RawCount>,
    pub capacity: Option<RawCapacity>,
    pub breakout_rooms: Option<RawCount>,
    pub price: Option<RawPrice>,
    pub catering_included: Option<RawFlag>,
    pub wheelchair_accessible: Option<RawFlag>,
    pub daylight: Option<String>,
    pub wifi: Option<String>,
    pub availability: Option<String>,
    pub notes: Option<String>,
}

/// `6`, `"~10 min"`, `"4-6"`, `"3 (one is the lounge)"`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawCount {
    Number(i64),
    Text(String),
}

/// `150`, `"80-120"`, `"90 seated / 130 standing"`, `{"plenary_theatre": 140, ...}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawCapacity {
    Number(i64),
    Text(String),
    Layouts(BTreeMap<String, i64>),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawPrice {
    Structured(RawStructuredPrice),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawStructuredPrice {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub amount: Option<RawAmount>,
    pub currency: Option<String>,
    pub vat: Option<String>,
    pub minimum_spend: Option<RawAmount>,
}

/// `9800`, `72.5` or `"7.900"`. Numbers and strings follow different decimal rules.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawAmount {
    Number(serde_json::Number),
    Text(String),
}

/// `true`, `false`, `"partial"`, `"ground floor only"`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawFlag {
    Bool(bool),
    Text(String),
}
