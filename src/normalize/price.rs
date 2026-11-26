//! N4: price, from a structured object or free text, into euro cents with an explicit VAT basis.

use super::words;
use crate::model::{Money, Price, PriceInfo, PriceUnit};
use crate::raw::{RawAmount, RawPrice, RawStructuredPrice};

pub const VAT_ASSUMED_RISK: &str = "VAT not stated, assumed excl.";
pub const DAY_PART_RISK: &str = "price per day-part, full day assumed = 2 day-parts";
pub const CURRENCY_ASSUMED_RISK: &str = "currency not stated, assumed EUR";

/// Returns the price and the assumptions made to read it. Risks are only returned for a
/// known price; an unknown price carries its reason instead.
pub fn parse_price(raw: Option<&RawPrice>) -> (PriceInfo, Vec<String>) {
    let parsed = match raw {
        None => Err("price not stated".to_string()),
        Some(RawPrice::Structured(price)) => parse_structured(price),
        Some(RawPrice::Text(text)) => parse_text(text),
    };
    match parsed {
        Ok((price, risks)) => (PriceInfo::Known(price), risks),
        Err(reason) => (PriceInfo::Unknown { reason }, Vec::new()),
    }
}

type Parsed = Result<(Price, Vec<String>), String>;

fn parse_structured(raw: &RawStructuredPrice) -> Parsed {
    let mut risks = Vec::new();

    match raw.currency.as_deref().map(str::trim) {
        None | Some("") => risks.push(CURRENCY_ASSUMED_RISK.to_string()),
        Some(c) if c.eq_ignore_ascii_case("EUR") => {}
        Some(other) => return Err(non_eur(other)),
    }

    let amount = match &raw.amount {
        None => return Err("price amount missing".to_string()),
        Some(amount) => {
            parse_amount(amount).ok_or_else(|| format!("price amount {amount:?} not understood"))?
        }
    };

    let unit = match raw.kind.as_deref().map(|k| k.trim().to_lowercase()) {
        None => return Err("price type not stated".to_string()),
        Some(kind) => match kind.as_str() {
            "per_day" => PriceUnit::PerDay,
            "per_person" => PriceUnit::PerPerson,
            "dagdeel" | "per_dagdeel" | "day_part" | "per_day_part" => PriceUnit::PerDayPart,
            _ => return Err(format!("price type \"{kind}\" not understood")),
        },
    };

    let vat_included = match raw.vat.as_deref() {
        None => {
            risks.push(VAT_ASSUMED_RISK.to_string());
            false
        }
        Some(vat) => match vat_basis(&vat.to_lowercase()) {
            Some(included) => included,
            None => {
                risks.push(format!("VAT basis \"{vat}\" not understood, assumed excl."));
                false
            }
        },
    };

    // An unreadable minimum spend cannot be dropped: ignoring it could understate the cost.
    let minimum_spend = match &raw.minimum_spend {
        None => None,
        Some(spend) => Some(
            parse_amount(spend).ok_or_else(|| format!("minimum spend {spend:?} not understood"))?,
        ),
    };

    Ok(finish(amount, unit, vat_included, minimum_spend, risks))
}

fn parse_text(text: &str) -> Parsed {
    let lower = text.trim().to_lowercase();
    let words = words(&lower);
    if lower.is_empty() {
        return Err("price not stated".to_string());
    }
    if lower.contains("on request") || lower.contains("op aanvraag") {
        return Err("price on request".to_string());
    }

    let mut risks = Vec::new();
    if lower.contains('$') || words.contains(&"usd") {
        return Err(non_eur("USD"));
    }
    if !lower.contains('€') && !words.contains(&"eur") {
        risks.push(CURRENCY_ASSUMED_RISK.to_string());
    }

    let not_understood = || format!("price text \"{}\" not understood", text.trim());
    let amount = amount_token(&lower)
        .and_then(parse_text_amount)
        .ok_or_else(not_understood)?;

    let unit = if lower.contains("dagdeel")
        || lower.contains("dagdelen")
        || lower.contains("day-part")
        || lower.contains("daypart")
    {
        PriceUnit::PerDayPart
    } else if lower.contains("per person")
        || lower.contains("per persoon")
        || lower.contains("p.p.")
        || words.contains(&"pp")
    {
        PriceUnit::PerPerson
    } else if lower.contains("per day") || lower.contains("per dag") {
        PriceUnit::PerDay
    } else {
        return Err(format!("price unit not stated in \"{}\"", text.trim()));
    };

    let vat_included = match words.iter().find_map(|w| vat_basis(w)) {
        Some(included) => included,
        None => {
            risks.push(VAT_ASSUMED_RISK.to_string());
            false
        }
    };

    Ok(finish(amount, unit, vat_included, None, risks))
}

fn finish(
    amount: Money,
    unit: PriceUnit,
    vat_included: bool,
    minimum_spend: Option<Money>,
    mut risks: Vec<String>,
) -> (Price, Vec<String>) {
    if unit == PriceUnit::PerDayPart {
        risks.push(DAY_PART_RISK.to_string());
    }
    let price = Price {
        amount,
        unit,
        vat_included,
        minimum_spend,
    };
    (price, risks)
}

/// We never invent an exchange rate.
fn non_eur(currency: &str) -> String {
    format!("priced in {}, no FX conversion", currency.to_uppercase())
}

/// `true` for incl. VAT, `false` for excl. ("ex", "excl", "exclusief"), `None` if neither.
fn vat_basis(word: &str) -> Option<bool> {
    match word.trim().trim_end_matches('.') {
        "incl" | "inc" | "including" | "inclusive" | "inclusief" => Some(true),
        "ex" | "excl" | "excluding" | "exclusive" | "exclusief" => Some(false),
        _ => None,
    }
}

fn parse_amount(raw: &RawAmount) -> Option<Money> {
    match raw {
        RawAmount::Number(n) => parse_number_amount(&n.to_string()),
        RawAmount::Text(text) => {
            let text = text.trim().trim_start_matches('€').trim();
            parse_text_amount(text)
        }
    }
}

/// A JSON number such as `72.5` is a plain decimal: `.` is always the decimal point here.
fn parse_number_amount(number: &str) -> Option<Money> {
    if number.contains(['e', 'E', '-']) {
        return None;
    }
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    to_cents(whole, fraction)
}

/// Dutch notation in text: `.` followed by exactly three digits is a thousands separator
/// ("7.900" is 7900 euros, not 7.9). A final `.` or `,` with one or two digits is the decimal
/// separator. Anything else (for example "7,900") is ambiguous and rejected.
fn parse_text_amount(token: &str) -> Option<Money> {
    let groups: Vec<&str> = token.split(['.', ',']).collect();
    let separators: Vec<char> = token.chars().filter(|c| matches!(c, '.' | ',')).collect();
    let mut whole = groups[0].to_string();
    let mut fraction = "";
    for (i, separator) in separators.iter().enumerate() {
        let group = groups[i + 1];
        let is_last = i + 1 == separators.len();
        if *separator == '.' && group.len() == 3 {
            whole.push_str(group);
        } else if is_last && (1..=2).contains(&group.len()) {
            fraction = group;
        } else {
            return None;
        }
    }
    to_cents(&whole, fraction)
}

/// Digits before and after the decimal point to cents, rounding half a cent up.
fn to_cents(whole: &str, fraction: &str) -> Option<Money> {
    let all_digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if whole.is_empty() || !all_digits(whole) || !all_digits(fraction) {
        return None;
    }
    let euros: i64 = whole.parse().ok()?;
    let digit = |i: usize| {
        fraction
            .as_bytes()
            .get(i)
            .map_or(0, |b| i64::from(b - b'0'))
    };
    let round_up = i64::from(digit(2) >= 5);
    let cents = euros
        .checked_mul(100)?
        .checked_add(digit(0) * 10 + digit(1) + round_up)?;
    Some(Money::from_cents(cents))
}

/// The first run of digits and separators in free text, e.g. "4.200" in "€4.200 per dagdeel".
fn amount_token(text: &str) -> Option<&str> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches(['.', ',']))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structured(json: &str) -> (PriceInfo, Vec<String>) {
        let raw: RawPrice = serde_json::from_str(json).unwrap();
        parse_price(Some(&raw))
    }

    fn text(s: &str) -> (PriceInfo, Vec<String>) {
        parse_price(Some(&RawPrice::Text(s.to_string())))
    }

    fn known(info: PriceInfo) -> Price {
        match info {
            PriceInfo::Known(price) => price,
            PriceInfo::Unknown { reason } => panic!("expected a known price, got: {reason}"),
        }
    }

    fn unknown_reason(info: PriceInfo) -> String {
        match info {
            PriceInfo::Unknown { reason } => reason,
            PriceInfo::Known(price) => panic!("expected an unknown price, got {price:?}"),
        }
    }

    #[test]
    fn structured_per_day_integer_amount() {
        let (info, risks) =
            structured(r#"{"type": "per_day", "amount": 9800, "currency": "EUR", "vat": "excl"}"#);
        let price = known(info);
        assert_eq!(price.amount, Money::from_euros(9800));
        assert_eq!(price.unit, PriceUnit::PerDay);
        assert!(!price.vat_included);
        assert_eq!(price.minimum_spend, None);
        assert!(risks.is_empty());
    }

    #[test]
    fn structured_per_person_decimal_number_is_plain() {
        let (info, _) = structured(
            r#"{"type": "per_person", "amount": 72.5, "currency": "EUR", "vat": "incl"}"#,
        );
        let price = known(info);
        assert_eq!(price.amount, Money::from_cents(7250));
        assert_eq!(price.unit, PriceUnit::PerPerson);
        assert!(price.vat_included);
    }

    #[test]
    fn structured_string_amount_uses_dutch_thousands() {
        let (info, _) = structured(
            r#"{"type": "per_day", "amount": "7.900", "currency": "EUR", "vat": "excl"}"#,
        );
        assert_eq!(known(info).amount, Money::from_euros(7900));
    }

    #[test]
    fn structured_minimum_spend_is_kept() {
        let (info, _) = structured(
            r#"{"type": "per_person", "amount": 95, "currency": "EUR", "vat": "excl", "minimum_spend": 10000}"#,
        );
        let price = known(info);
        assert_eq!(price.amount, Money::from_euros(95));
        assert_eq!(price.minimum_spend, Some(Money::from_euros(10000)));
    }

    #[test]
    fn structured_usd_is_unknown() {
        let (info, risks) = structured(
            r#"{"type": "per_person", "amount": 110, "currency": "USD", "vat": "excl"}"#,
        );
        assert_eq!(unknown_reason(info), "priced in USD, no FX conversion");
        assert!(risks.is_empty());
    }

    #[test]
    fn structured_missing_vat_is_excl_with_risk() {
        let (info, risks) = structured(r#"{"type": "per_day", "amount": 5000, "currency": "EUR"}"#);
        assert!(!known(info).vat_included);
        assert_eq!(risks, vec![VAT_ASSUMED_RISK]);
    }

    #[test]
    fn structured_missing_or_bad_parts_are_unknown() {
        for json in [
            r#"{"type": "per_day", "currency": "EUR", "vat": "excl"}"#,
            r#"{"amount": 5000, "currency": "EUR", "vat": "excl"}"#,
            r#"{"type": "per_week", "amount": 5000, "currency": "EUR", "vat": "excl"}"#,
            r#"{"type": "per_day", "amount": "lots", "currency": "EUR", "vat": "excl"}"#,
            r#"{"type": "per_day", "amount": -5, "currency": "EUR", "vat": "excl"}"#,
            r#"{"type": "per_person", "amount": 95, "currency": "EUR", "vat": "excl", "minimum_spend": "?"}"#,
        ] {
            assert!(
                matches!(structured(json).0, PriceInfo::Unknown { .. }),
                "{json}"
            );
        }
    }

    #[test]
    fn free_text_day_part_excl_vat() {
        let (info, risks) = text("€4.200 per dagdeel ex. BTW");
        let price = known(info);
        assert_eq!(price.amount, Money::from_euros(4200));
        assert_eq!(price.unit, PriceUnit::PerDayPart);
        assert!(!price.vat_included);
        assert_eq!(risks, vec![DAY_PART_RISK]);
    }

    #[test]
    fn free_text_without_vat_is_excl_with_risk() {
        let (info, risks) = text("€4.200 per dagdeel");
        assert!(!known(info).vat_included);
        assert_eq!(risks, vec![VAT_ASSUMED_RISK, DAY_PART_RISK]);
    }

    #[test]
    fn free_text_incl_vat_per_person() {
        let price = known(text("€72,50 pp incl. BTW").0);
        assert_eq!(price.amount, Money::from_cents(7250));
        assert_eq!(price.unit, PriceUnit::PerPerson);
        assert!(price.vat_included);
    }

    #[test]
    fn free_text_on_request_or_unparseable_is_unknown() {
        assert_eq!(unknown_reason(text("on request").0), "price on request");
        assert!(matches!(text("cheap").0, PriceInfo::Unknown { .. }));
        assert!(matches!(text("€4.200").0, PriceInfo::Unknown { .. }));
        assert!(matches!(
            text("$4,200 per day").0,
            PriceInfo::Unknown { .. }
        ));
    }

    #[test]
    fn null_price_is_unknown() {
        assert_eq!(unknown_reason(parse_price(None).0), "price not stated");
    }

    #[test]
    fn dutch_text_amounts() {
        let cases = [
            ("7.900", Some(790_000)),
            ("4.200", Some(420_000)),
            ("1.234.567,89", Some(123_456_789)),
            ("72.5", Some(7_250)),
            ("72,50", Some(7_250)),
            ("9800", Some(980_000)),
            ("7,900", None),
            ("7.9000", None),
            ("1,234.56", None),
            ("", None),
        ];
        for (input, cents) in cases {
            assert_eq!(
                parse_text_amount(input),
                cents.map(Money::from_cents),
                "{input}"
            );
        }
    }

    #[test]
    fn number_amounts_round_half_up_to_cents() {
        assert_eq!(parse_number_amount("72.5"), Some(Money::from_cents(7250)));
        assert_eq!(parse_number_amount("0.125"), Some(Money::from_cents(13)));
        assert_eq!(parse_number_amount("0.124"), Some(Money::from_cents(12)));
        assert_eq!(parse_number_amount("1e3"), None);
    }
}
