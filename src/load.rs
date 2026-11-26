//! Reads the venue export. Only an unreadable or non-JSON file is an error; a bad record is a
//! warning and is skipped (N10).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::Venue;
use crate::normalize::normalize_venue;
use crate::raw::{RawFile, RawVenue};

#[derive(Debug)]
pub enum LoadError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    /// Not JSON, or JSON without a `venues` list.
    Json(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            LoadError::Json(source) => write!(f, "not a valid venue file: {source}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Read { source, .. } => Some(source),
            LoadError::Json(source) => Some(source),
        }
    }
}

#[derive(Debug)]
pub struct Loaded {
    pub venues: Vec<Venue>,
    /// One line per skipped record, naming the record and why.
    pub warnings: Vec<String>,
}

pub fn load_venues(path: &Path) -> Result<Loaded, LoadError> {
    let json = fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_venues(&json)
}

pub fn parse_venues(json: &str) -> Result<Loaded, LoadError> {
    let file: RawFile = serde_json::from_str(json).map_err(LoadError::Json)?;
    let mut venues = Vec::new();
    let mut warnings = Vec::new();
    for (index, record) in file.venues.into_iter().enumerate() {
        let label = match record.get("id").and_then(Value::as_str) {
            Some(id) => format!("{id} (record {})", index + 1),
            None => format!("record {}", index + 1),
        };
        let normalized = serde_json::from_value::<RawVenue>(record)
            .map_err(|e| format!("malformed record: {e}"))
            .and_then(normalize_venue);
        match normalized {
            Ok(venue) => venues.push(venue),
            Err(reason) => warnings.push(format!("skipped {label}: {reason}")),
        }
    }
    Ok(Loaded { venues, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Capacity, Money, PriceInfo};

    #[test]
    fn malformed_records_are_warnings_not_errors() {
        let json = r#"{"venues": [
            {"id": "ok", "name": "Fine"},
            {"id": "bad", "name": "Broken", "capacity": true},
            42,
            {"name": "No id"}
        ]}"#;
        let loaded = parse_venues(json).unwrap();
        assert_eq!(loaded.venues.len(), 1);
        assert_eq!(loaded.venues[0].ids, vec!["ok".to_string()]);
        assert_eq!(loaded.warnings.len(), 3, "{:?}", loaded.warnings);
        assert!(loaded.warnings[0].starts_with("skipped bad (record 2): malformed record"));
        assert!(loaded.warnings[1].starts_with("skipped record 3"));
        assert_eq!(loaded.warnings[2], "skipped record 4: missing id");
    }

    #[test]
    fn non_json_or_wrong_shape_is_an_error() {
        assert!(matches!(parse_venues("not json"), Err(LoadError::Json(_))));
        assert!(matches!(parse_venues("{}"), Err(LoadError::Json(_))));
    }

    #[test]
    fn missing_file_is_a_read_error() {
        let err = load_venues(Path::new("does/not/exist.json")).unwrap_err();
        assert!(matches!(err, LoadError::Read { .. }));
    }

    #[test]
    fn real_dataset_loads_without_warnings() {
        let loaded = parse_venues(include_str!("../venues.json")).unwrap();
        assert_eq!(loaded.venues.len(), 30);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);

        let venue = |id: &str| {
            loaded
                .venues
                .iter()
                .find(|v| v.ids[0] == id)
                .unwrap_or_else(|| panic!("{id} missing"))
        };
        match &venue("v008").price {
            PriceInfo::Known(price) => assert_eq!(price.amount, Money::from_euros(7900)),
            other => panic!("v008 price: {other:?}"),
        }
        assert_eq!(venue("v027").capacity, Capacity::Unknown);
        assert_eq!(
            venue("v029").price,
            PriceInfo::Unknown {
                reason: "priced in USD, no FX conversion".to_string()
            }
        );
    }
}
