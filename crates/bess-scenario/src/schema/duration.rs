//! ISO 8601 durations, the subset a scenario needs: `PnW`, or `PnD`
//! followed by `T` and any of `nH`, `nM`, `nS`, in whole seconds.
//!
//! Five tokens, so the crate's own and no date-time dependency (design D1).
//! Months and years are refused because their length depends on the
//! calendar a run is in; weeks stand alone, as the standard has them.

/// Why a duration was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DurationError {
    /// The text does not start with `P`.
    #[error("{0:?} is not an ISO 8601 duration: it starts with P (PT14H, P2D, P1W)")]
    MissingP(String),
    /// A minus sign: offsets run forward from `start`.
    #[error("{0:?}: a duration cannot be negative")]
    Negative(String),
    /// A decimal point or comma.
    #[error("{0:?}: whole seconds only, no fractions")]
    Fraction(String),
    /// `Y`, or `M` before `T`.
    #[error("{0:?}: months and years are refused, their length depends on the calendar; use days")]
    MonthsOrYears(String),
    /// `H` or `S` before `T`.
    #[error("{0:?}: hours, minutes and seconds follow a T (PT14H, P1DT6H)")]
    MissingT(String),
    /// Weeks combined with other parts.
    #[error("{0:?}: weeks stand alone (P2W); write other spans in days")]
    WeeksCombined(String),
    /// `P` or `PT` with nothing after it.
    #[error("{0:?} has no parts; zero is PT0S")]
    Empty(String),
    /// Anything else: a unit out of order or repeated, a number without a
    /// unit, a stray character.
    #[error(
        "{0:?} is not a duration this grammar reads (PnW, or PnDTnHnMnS with any of the parts)"
    )]
    Malformed(String),
    /// More seconds than a u64 holds.
    #[error("{0:?} is too long")]
    Overflow(String),
}

/// Seconds in a duration.
pub fn parse(text: &str) -> Result<u64, DurationError> {
    let own = || text.to_owned();
    if text.starts_with('-') {
        return Err(DurationError::Negative(own()));
    }
    let Some(rest) = text.strip_prefix('P') else {
        return Err(DurationError::MissingP(own()));
    };
    if rest.contains(['.', ',']) {
        return Err(DurationError::Fraction(own()));
    }
    let (date, time) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    if date.contains(['Y', 'M']) {
        return Err(DurationError::MonthsOrYears(own()));
    }
    if date.contains(['H', 'S']) {
        return Err(DurationError::MissingT(own()));
    }
    if date.contains('W') && (date.contains('D') || time.is_some()) {
        return Err(DurationError::WeeksCombined(own()));
    }
    if date.is_empty() && time.is_none_or(str::is_empty) {
        return Err(DurationError::Empty(own()));
    }
    if time == Some("") {
        return Err(DurationError::Malformed(own()));
    }

    let mut total: u64 = 0;
    let mut add = |part: &str, units: &[(char, u64)]| -> Result<(), DurationError> {
        let mut next_unit = 0;
        let mut digits = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
                continue;
            }
            let pos = units[next_unit..]
                .iter()
                .position(|&(u, _)| u == c)
                .ok_or_else(|| DurationError::Malformed(own()))?;
            if digits.is_empty() {
                return Err(DurationError::Malformed(own()));
            }
            let (_, scale) = units[next_unit + pos];
            let n: u64 = digits.parse().map_err(|_| DurationError::Overflow(own()))?;
            total = n
                .checked_mul(scale)
                .and_then(|s| total.checked_add(s))
                .ok_or_else(|| DurationError::Overflow(own()))?;
            digits.clear();
            next_unit += pos + 1;
        }
        if digits.is_empty() {
            Ok(())
        } else {
            Err(DurationError::Malformed(own()))
        }
    };
    add(date, &[('W', 7 * 86_400), ('D', 86_400)])?;
    if let Some(t) = time {
        add(t, &[('H', 3600), ('M', 60), ('S', 1)])?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::{parse, DurationError as E};

    #[test]
    fn accepted_forms() {
        assert_eq!(parse("PT0S"), Ok(0));
        assert_eq!(parse("PT14H"), Ok(14 * 3600));
        assert_eq!(parse("PT14H20M"), Ok(14 * 3600 + 20 * 60));
        assert_eq!(parse("PT90M"), Ok(5400));
        assert_eq!(parse("PT1H0M30S"), Ok(3630));
        assert_eq!(parse("P2D"), Ok(2 * 86_400));
        assert_eq!(parse("P9DT6H"), Ok(9 * 86_400 + 6 * 3600));
        assert_eq!(parse("P1DT1S"), Ok(86_401));
        assert_eq!(parse("P1W"), Ok(7 * 86_400));
        assert_eq!(parse("P52W"), Ok(52 * 7 * 86_400));
    }

    #[test]
    fn refusals_name_their_rule() {
        type Rule = fn(String) -> E;
        let cases: [(&str, Rule); 14] = [
            ("14:00", E::MissingP),
            ("-PT1H", E::Negative),
            ("PT1.5H", E::Fraction),
            ("PT0,5S", E::Fraction),
            ("P1M", E::MonthsOrYears),
            ("P1Y", E::MonthsOrYears),
            ("P1H", E::MissingT),
            ("P1W2D", E::WeeksCombined),
            ("P1WT1H", E::WeeksCombined),
            ("P", E::Empty),
            ("PT", E::Empty),
            ("PT5M3H", E::Malformed),
            ("PTH", E::Malformed),
            ("P99999999999999999999D", E::Overflow),
        ];
        for (text, want) in cases {
            assert_eq!(parse(text), Err(want(text.to_owned())), "{text}");
        }
        assert_eq!(parse("P1DT"), Err(E::Malformed("P1DT".to_owned())));
        assert_eq!(parse("PT1H1H"), Err(E::Malformed("PT1H1H".to_owned())));
        assert_eq!(parse("PT12"), Err(E::Malformed("PT12".to_owned())));
    }
}
