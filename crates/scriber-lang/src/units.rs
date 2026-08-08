//! Dimensions, quantities, and the arithmetic table.
//!
//! Lengths normalise to millimetres and angles to radians at the literal, so
//! everything downstream sees one representation.

/// The three dimensions a value can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Length,
    Angle,
    Scalar,
}

impl Dimension {
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Length => "length",
            Dimension::Angle => "angle",
            Dimension::Scalar => "scalar",
        }
    }
}

/// A value together with its dimension. `value` is millimetres for a length
/// and radians for an angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub dimension: Dimension,
}

/// A dimension rule violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimError {
    pub message: String,
}

impl DimError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Failure while reading a numeric literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitError {
    UnknownUnit(String),
    MalformedNumber(String),
}

/// The length units a document may declare or a literal may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Mm,
    Cm,
    M,
    In,
    Ft,
}

impl LengthUnit {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mm" => Some(LengthUnit::Mm),
            "cm" => Some(LengthUnit::Cm),
            "m" => Some(LengthUnit::M),
            "in" => Some(LengthUnit::In),
            "ft" => Some(LengthUnit::Ft),
            _ => None,
        }
    }

    pub fn to_mm(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Cm => 10.0,
            LengthUnit::M => 1000.0,
            LengthUnit::In => 25.4,
            LengthUnit::Ft => 304.8,
        }
    }
}

/// Splits a numeric literal into value and optional unit suffix.
///
/// The lexer keeps `12mm` as one token precisely so this split happens where
/// a good error message can be produced.
///
/// A bare number stays a [`Dimension::Scalar`]; the document's default unit is
/// applied later, by [`Quantity::coerce_to`], only where a length is actually
/// wanted. `_default` is therefore unused today and kept only so callers do not
/// have to change when a future literal form needs it.
///
/// The number part is deliberately not exponent or hex notation: `1e5` splits
/// into `1` and the suffix `e5` and is rejected as an unknown unit rather than
/// silently read as 100000. The lexer cannot produce such a token in one piece
/// anyway — it lexes `1e5` as `1e` then `5` — so accepting it here would make
/// the two disagree.
pub fn parse_number(text: &str, _default: LengthUnit) -> Result<Quantity, UnitError> {
    let split = text
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (number, suffix) = text.split_at(split);

    let value: f64 = number
        .parse()
        .map_err(|_| UnitError::MalformedNumber(text.to_string()))?;

    if suffix.is_empty() {
        return Ok(Quantity {
            value,
            dimension: Dimension::Scalar,
        });
    }

    if let Some(unit) = LengthUnit::parse(suffix) {
        return Ok(Quantity {
            value: value * unit.to_mm(),
            dimension: Dimension::Length,
        });
    }

    match suffix {
        "deg" => Ok(Quantity {
            value: value.to_radians(),
            dimension: Dimension::Angle,
        }),
        "rad" => Ok(Quantity {
            value,
            dimension: Dimension::Angle,
        }),
        _ => Err(UnitError::UnknownUnit(suffix.to_string())),
    }
}

impl Quantity {
    /// Reinterprets a scalar as a length in document units. Any other
    /// conversion is rejected.
    pub fn coerce_to(self, target: Dimension, default: LengthUnit) -> Result<Quantity, DimError> {
        if self.dimension == target {
            return Ok(self);
        }

        if self.dimension == Dimension::Scalar && target == Dimension::Length {
            return Ok(Quantity {
                value: self.value * default.to_mm(),
                dimension: Dimension::Length,
            });
        }

        Err(DimError::new(format!(
            "expected {}, found {}",
            target.name(),
            self.dimension.name()
        )))
    }

    pub fn add(self, rhs: Quantity, default: LengthUnit) -> Result<Quantity, DimError> {
        self.additive(rhs, default, "add", |a, b| a + b)
    }

    pub fn sub(self, rhs: Quantity, default: LengthUnit) -> Result<Quantity, DimError> {
        self.additive(rhs, default, "subtract", |a, b| a - b)
    }

    fn additive(
        self,
        rhs: Quantity,
        default: LengthUnit,
        verb: &str,
        apply: fn(f64, f64) -> f64,
    ) -> Result<Quantity, DimError> {
        use Dimension::{Length, Scalar};

        let (lhs, rhs) = match (self.dimension, rhs.dimension) {
            (a, b) if a == b => (self, rhs),
            // A scalar reads as a length, because `units` declares a default
            // length unit. There is no default angle unit, so angles do not
            // get the same treatment.
            (Length, Scalar) => (self, rhs.coerce_to(Length, default)?),
            (Scalar, Length) => (self.coerce_to(Length, default)?, rhs),
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot {verb} {} and {}",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity {
            value: apply(lhs.value, rhs.value),
            dimension: lhs.dimension,
        })
    }

    pub fn mul(self, rhs: Quantity, _default: LengthUnit) -> Result<Quantity, DimError> {
        use Dimension::Scalar;

        let dimension = match (self.dimension, rhs.dimension) {
            (Scalar, other) | (other, Scalar) => other,
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot multiply {} by {} — no operation takes a derived dimension",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity {
            value: self.value * rhs.value,
            dimension,
        })
    }

    pub fn div(self, rhs: Quantity, _default: LengthUnit) -> Result<Quantity, DimError> {
        use Dimension::Scalar;

        if rhs.value == 0.0 {
            return Err(DimError::new("division by zero"));
        }

        let dimension = match (self.dimension, rhs.dimension) {
            (a, b) if a == b => Scalar,
            (other, Scalar) => other,
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot divide {} by {}",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity {
            value: self.value / rhs.value,
            dimension,
        })
    }

    // Deliberately an inherent method rather than `std::ops::Neg`: the
    // evaluator calls it on a `Result`-producing path beside `add`/`sub`/`mul`,
    // and matching their shape reads better there than an operator would.
    #[allow(clippy::should_implement_trait)]
    pub fn neg(self) -> Quantity {
        Quantity {
            value: -self.value,
            dimension: self.dimension,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(value: f64, dimension: Dimension) -> Quantity {
        Quantity { value, dimension }
    }

    #[test]
    fn parses_a_bare_number_as_a_scalar() {
        let parsed = parse_number("60", LengthUnit::Mm).unwrap();
        assert_eq!(parsed.dimension, Dimension::Scalar);
        assert_eq!(parsed.value, 60.0);
    }

    #[test]
    fn normalises_lengths_to_millimetres() {
        assert_eq!(parse_number("1cm", LengthUnit::Mm).unwrap().value, 10.0);
        assert_eq!(parse_number("1in", LengthUnit::Mm).unwrap().value, 25.4);
        assert_eq!(
            parse_number("1m", LengthUnit::Mm).unwrap().dimension,
            Dimension::Length
        );
    }

    #[test]
    fn normalises_angles_to_radians() {
        let parsed = parse_number("180deg", LengthUnit::Mm).unwrap();
        assert_eq!(parsed.dimension, Dimension::Angle);
        assert!((parsed.value - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn rejects_an_unknown_suffix() {
        assert!(matches!(
            parse_number("5furlong", LengthUnit::Mm),
            Err(UnitError::UnknownUnit(_))
        ));
    }

    #[test]
    fn a_scalar_reads_as_a_length_when_added_to_one() {
        // `units mm` declares a default *length* unit, so 1 means 1mm here.
        let sum = q(5.0, Dimension::Length)
            .add(q(1.0, Dimension::Scalar), LengthUnit::Mm)
            .unwrap();
        assert_eq!(sum.dimension, Dimension::Length);
        assert_eq!(sum.value, 6.0);
    }

    #[test]
    fn a_scalar_does_not_read_as_an_angle() {
        // There is no declared default angle unit, so this must stay an error.
        assert!(
            q(1.0, Dimension::Angle)
                .add(q(1.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn multiplying_two_lengths_is_rejected() {
        assert!(
            q(2.0, Dimension::Length)
                .mul(q(3.0, Dimension::Length), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn scaling_a_length_keeps_it_a_length() {
        let scaled = q(2.0, Dimension::Length)
            .mul(q(3.0, Dimension::Scalar), LengthUnit::Mm)
            .unwrap();
        assert_eq!(scaled.dimension, Dimension::Length);
        assert_eq!(scaled.value, 6.0);
    }

    #[test]
    fn dividing_like_by_like_yields_a_scalar() {
        let ratio = q(6.0, Dimension::Length)
            .div(q(2.0, Dimension::Length), LengthUnit::Mm)
            .unwrap();
        assert_eq!(ratio.dimension, Dimension::Scalar);
        assert_eq!(ratio.value, 3.0);
    }

    #[test]
    fn dividing_a_scalar_by_a_length_is_rejected() {
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(2.0, Dimension::Length), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn dividing_by_zero_is_an_error_not_an_infinity() {
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(0.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn coercing_a_scalar_to_a_length_uses_document_units() {
        let coerced = q(12.0, Dimension::Scalar)
            .coerce_to(Dimension::Length, LengthUnit::Cm)
            .unwrap();
        assert_eq!(coerced.value, 120.0);
        assert_eq!(coerced.dimension, Dimension::Length);
    }

    #[test]
    fn coercing_an_angle_to_a_length_is_rejected() {
        assert!(
            q(1.0, Dimension::Angle)
                .coerce_to(Dimension::Length, LengthUnit::Mm)
                .is_err()
        );
    }

    // Edge cases beyond the brief's table.

    #[test]
    fn exponent_and_hex_notation_are_rejected_rather_than_reinterpreted() {
        // The lexer's number regex has neither exponents nor hex, so it hands
        // `1e5` over as `1e` then `5`. Accepting `1e5` here would make the two
        // disagree, and `f64::from_str` would read it as 100000 — a silent
        // hundred-thousandfold error in a CAD dimension. Reject it instead.
        assert_eq!(
            parse_number("1e5", LengthUnit::Mm),
            Err(UnitError::UnknownUnit("e5".to_string()))
        );
        assert_eq!(
            parse_number("1e", LengthUnit::Mm),
            Err(UnitError::UnknownUnit("e".to_string()))
        );
        assert_eq!(
            parse_number("0x1f", LengthUnit::Mm),
            Err(UnitError::UnknownUnit("x1f".to_string()))
        );
    }

    #[test]
    fn a_literal_with_no_digits_is_malformed_not_unknown() {
        // `mm` alone reaches the lexer as an Ident, but the function is public
        // and must not panic or read an empty string as zero.
        assert_eq!(
            parse_number("mm", LengthUnit::Mm),
            Err(UnitError::MalformedNumber("mm".to_string()))
        );
        assert_eq!(
            parse_number("", LengthUnit::Mm),
            Err(UnitError::MalformedNumber(String::new()))
        );
        assert_eq!(
            parse_number("1.2.3", LengthUnit::Mm),
            Err(UnitError::MalformedNumber("1.2.3".to_string()))
        );
    }

    #[test]
    fn the_words_inf_and_nan_are_not_numbers() {
        // `f64::from_str` accepts both. The split runs first, so they land in
        // the suffix and never become a value.
        assert_eq!(
            parse_number("inf", LengthUnit::Mm),
            Err(UnitError::MalformedNumber("inf".to_string()))
        );
        assert_eq!(
            parse_number("NaN", LengthUnit::Mm),
            Err(UnitError::MalformedNumber("NaN".to_string()))
        );
    }

    #[test]
    fn unit_suffixes_are_case_sensitive() {
        assert_eq!(
            parse_number("5MM", LengthUnit::Mm),
            Err(UnitError::UnknownUnit("MM".to_string()))
        );
        assert_eq!(
            parse_number("180DEG", LengthUnit::Mm),
            Err(UnitError::UnknownUnit("DEG".to_string()))
        );
    }

    #[test]
    fn a_non_ascii_digit_does_not_split_mid_character() {
        // The split point comes from an ASCII search, so it is always a char
        // boundary and `split_at` cannot panic on multi-byte input.
        assert_eq!(
            parse_number("٣mm", LengthUnit::Mm),
            Err(UnitError::MalformedNumber("٣mm".to_string()))
        );
    }

    #[test]
    fn dividing_by_negative_zero_is_still_division_by_zero() {
        // `-0.0 == 0.0` is true for f64, so the guard catches both signs and
        // no expression can produce a negative infinity this way.
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(-0.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn subtraction_follows_the_same_coercion_rule_as_addition() {
        let difference = q(5.0, Dimension::Length)
            .sub(q(1.0, Dimension::Scalar), LengthUnit::Cm)
            .unwrap();
        assert_eq!(difference.dimension, Dimension::Length);
        assert_eq!(difference.value, -5.0);
        assert!(
            q(1.0, Dimension::Angle)
                .sub(q(1.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn angles_multiply_and_divide_by_the_same_rules_as_lengths() {
        assert!(
            q(2.0, Dimension::Angle)
                .mul(q(3.0, Dimension::Angle), LengthUnit::Mm)
                .is_err()
        );
        assert_eq!(
            q(6.0, Dimension::Angle)
                .div(q(2.0, Dimension::Angle), LengthUnit::Mm)
                .unwrap()
                .dimension,
            Dimension::Scalar
        );
        assert_eq!(
            q(6.0, Dimension::Angle)
                .div(q(2.0, Dimension::Scalar), LengthUnit::Mm)
                .unwrap()
                .dimension,
            Dimension::Angle
        );
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(2.0, Dimension::Angle), LengthUnit::Mm)
                .is_err()
        );
        assert!(
            q(1.0, Dimension::Length)
                .div(q(2.0, Dimension::Angle), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn negation_keeps_the_dimension() {
        let negated = q(3.0, Dimension::Length).neg();
        assert_eq!(negated.value, -3.0);
        assert_eq!(negated.dimension, Dimension::Length);
    }

    #[test]
    fn every_length_unit_round_trips_through_parse() {
        for (text, unit, mm) in [
            ("mm", LengthUnit::Mm, 1.0),
            ("cm", LengthUnit::Cm, 10.0),
            ("m", LengthUnit::M, 1000.0),
            ("in", LengthUnit::In, 25.4),
            ("ft", LengthUnit::Ft, 304.8),
        ] {
            assert_eq!(LengthUnit::parse(text), Some(unit));
            assert_eq!(unit.to_mm(), mm);
            assert_eq!(
                parse_number(&format!("2{text}"), LengthUnit::Mm)
                    .unwrap()
                    .value,
                2.0 * mm
            );
        }
        assert_eq!(LengthUnit::parse("furlong"), None);
    }

    // The two known gaps below are pinned so that closing them is a deliberate
    // change with a failing test, not an accident.

    #[test]
    fn an_overflowing_literal_becomes_infinity_rather_than_an_error() {
        // `f64::from_str` saturates to infinity instead of failing, so a
        // 400-digit literal parses. Task 8 should reject non-finite values
        // where geometry is built; this module does not know about that yet.
        let huge = "1".repeat(400);
        assert!(
            parse_number(&huge, LengthUnit::Mm)
                .unwrap()
                .value
                .is_infinite()
        );
    }

    #[test]
    fn a_nan_divisor_slips_past_the_zero_guard() {
        // `NaN == 0.0` is false, so the guard does not fire and the NaN
        // propagates. Nothing in this module can produce a NaN from a literal,
        // so the only route in is an evaluator that already has one.
        let nan = q(f64::NAN, Dimension::Scalar);
        let result = q(1.0, Dimension::Scalar).div(nan, LengthUnit::Mm).unwrap();
        assert!(result.value.is_nan());
        // Dividing by an infinity is likewise allowed and gives zero.
        let infinite = q(f64::INFINITY, Dimension::Scalar);
        assert_eq!(
            q(1.0, Dimension::Scalar)
                .div(infinite, LengthUnit::Mm)
                .unwrap()
                .value,
            0.0
        );
    }
}
