//! Physical units with dimensional analysis
//!
//! Supports compound units like m², km/h, m/s² through dimensional tracking.
//! Each unit has a scale factor and dimension exponents.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::EvalError;

/// Tolerance for unit conversion factor matching (0.1%)
const CONVERSION_TOLERANCE: &str = "0.001";

/// Helper to create Decimal from string (panics on invalid input, only for static definitions)
fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

// ============================================================================
// DIMENSIONS
// ============================================================================

/// SI base dimensions as signed exponents
/// Examples:
/// - meter: length=1
/// - m²: length=2
/// - m/s: length=1, time=-1
/// - N (kg·m/s²): mass=1, length=1, time=-2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Dimensions {
    pub length: i8,      // L (meter)
    pub mass: i8,        // M (kilogram)
    pub time: i8,        // T (second)
    pub temperature: i8, // Θ (kelvin/celsius)
    pub data: i8,        // D (byte) - non-SI but useful
    pub angle: i8,       // A (radian/degree semantic dimension)
}

impl Dimensions {
    /// Create dimensions with only length
    pub const fn length(exp: i8) -> Self {
        Self {
            length: exp,
            ..Self::ZERO
        }
    }

    /// Create dimensions with only mass
    pub const fn mass(exp: i8) -> Self {
        Self {
            mass: exp,
            ..Self::ZERO
        }
    }

    /// Create dimensions with only time
    pub const fn time(exp: i8) -> Self {
        Self {
            time: exp,
            ..Self::ZERO
        }
    }

    /// Create dimensions with only temperature
    pub const fn temperature(exp: i8) -> Self {
        Self {
            temperature: exp,
            ..Self::ZERO
        }
    }

    /// Create dimensions with only data
    pub const fn data(exp: i8) -> Self {
        Self {
            data: exp,
            ..Self::ZERO
        }
    }

    /// Create dimensions with only plane angle.
    pub const fn angle(exp: i8) -> Self {
        Self {
            angle: exp,
            ..Self::ZERO
        }
    }

    /// Dimensionless (all zeros)
    pub const ZERO: Self = Self {
        length: 0,
        mass: 0,
        time: 0,
        temperature: 0,
        data: 0,
        angle: 0,
    };

    /// Multiply dimensions (add exponents), returning `None` on overflow.
    pub fn checked_multiply(self, other: Self) -> Option<Self> {
        Some(Self {
            length: self.length.checked_add(other.length)?,
            mass: self.mass.checked_add(other.mass)?,
            time: self.time.checked_add(other.time)?,
            temperature: self.temperature.checked_add(other.temperature)?,
            data: self.data.checked_add(other.data)?,
            angle: self.angle.checked_add(other.angle)?,
        })
    }

    /// Divide dimensions (subtract exponents), returning `None` on overflow.
    pub fn checked_divide(self, other: Self) -> Option<Self> {
        Some(Self {
            length: self.length.checked_sub(other.length)?,
            mass: self.mass.checked_sub(other.mass)?,
            time: self.time.checked_sub(other.time)?,
            temperature: self.temperature.checked_sub(other.temperature)?,
            data: self.data.checked_sub(other.data)?,
            angle: self.angle.checked_sub(other.angle)?,
        })
    }

    /// Raise dimensions to a power, returning `None` on overflow.
    pub fn checked_power(self, exp: i8) -> Option<Self> {
        Some(Self {
            length: self.length.checked_mul(exp)?,
            mass: self.mass.checked_mul(exp)?,
            time: self.time.checked_mul(exp)?,
            temperature: self.temperature.checked_mul(exp)?,
            data: self.data.checked_mul(exp)?,
            angle: self.angle.checked_mul(exp)?,
        })
    }

    /// Check if dimensionless
    pub fn is_dimensionless(&self) -> bool {
        *self == Self::ZERO
    }

    /// Check if dimensions are compatible (same or one is dimensionless)
    pub fn is_compatible(&self, other: &Self) -> bool {
        *self == *other || self.is_dimensionless() || other.is_dimensionless()
    }
}

// ============================================================================
// COMPOUND UNIT
// ============================================================================

/// A unit with scale factor and dimensions
/// Can represent simple units (m, kg) or compound units (m/s, km/h, m²)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompoundUnit {
    /// Conversion factor to SI base units
    pub factor: Decimal,
    /// Offset for non-linear conversions (used for temperature)
    pub offset: Decimal,
    /// Dimensional exponents
    pub dimensions: Dimensions,
    /// Display name (e.g., "km", "m/s", "m²")
    pub symbol: String,
}

impl CompoundUnit {
    /// Create a new compound unit
    pub fn new(factor: Decimal, dimensions: Dimensions, symbol: impl Into<String>) -> Self {
        Self {
            factor,
            offset: Decimal::ZERO,
            dimensions,
            symbol: symbol.into(),
        }
    }

    /// Create with offset (for temperature conversions)
    pub fn with_offset(
        factor: Decimal,
        offset: Decimal,
        dimensions: Dimensions,
        symbol: impl Into<String>,
    ) -> Self {
        Self {
            factor,
            offset,
            dimensions,
            symbol: symbol.into(),
        }
    }

    /// Convert a value to SI base units, returning `None` on overflow.
    pub fn checked_to_si(&self, value: Decimal) -> Option<Decimal> {
        value.checked_add(self.offset)?.checked_mul(self.factor)
    }

    /// Convert a value from SI base units, returning `None` on overflow.
    pub fn checked_from_si(&self, si_value: Decimal) -> Option<Decimal> {
        si_value.checked_div(self.factor)?.checked_sub(self.offset)
    }

    /// Multiply two units (for operations like 5m * 10m = 50m²).
    pub fn try_multiply(&self, other: &Self) -> Result<Self, EvalError> {
        let new_dims =
            self.dimensions
                .checked_multiply(other.dimensions)
                .ok_or(EvalError::Overflow {
                    operation: "combining unit dimensions",
                })?;
        let new_factor = self
            .factor
            .checked_mul(other.factor)
            .ok_or(EvalError::Overflow {
                operation: "combining unit scales",
            })?;
        Ok(Self {
            factor: new_factor,
            offset: Decimal::ZERO,
            dimensions: new_dims,
            symbol: smart_symbol(&self.symbol, &other.symbol, &new_dims, new_factor, true),
        })
    }

    /// Divide two units (for operations like 100km / 2h = 50km/h).
    pub fn try_divide(&self, other: &Self) -> Result<Self, EvalError> {
        let new_dims =
            self.dimensions
                .checked_divide(other.dimensions)
                .ok_or(EvalError::Overflow {
                    operation: "combining unit dimensions",
                })?;
        let new_factor = self
            .factor
            .checked_div(other.factor)
            .ok_or(EvalError::Overflow {
                operation: "combining unit scales",
            })?;
        Ok(Self {
            factor: new_factor,
            offset: Decimal::ZERO,
            dimensions: new_dims,
            symbol: smart_symbol(&self.symbol, &other.symbol, &new_dims, new_factor, false),
        })
    }

    /// Check if this unit can be converted to another
    pub fn can_convert_to(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions
    }

    /// Convert a value from this unit to another unit.
    pub fn try_convert_to(
        &self,
        value: Decimal,
        target: &Self,
    ) -> Result<Option<Decimal>, EvalError> {
        if !self.can_convert_to(target) {
            return Ok(None);
        }
        let si_value = self.checked_to_si(value).ok_or(EvalError::Overflow {
            operation: "converting a unit to its base scale",
        })?;
        let converted = target
            .checked_from_si(si_value)
            .ok_or(EvalError::Overflow {
                operation: "converting a unit from its base scale",
            })?;
        Ok(Some(converted))
    }
}

impl fmt::Display for CompoundUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

impl Eq for CompoundUnit {}

impl std::hash::Hash for CompoundUnit {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.symbol.hash(state);
        self.dimensions.hash(state);
    }
}

// ============================================================================
// UNIT REGISTRY
// ============================================================================

// Runtime-initialized unit definitions with proper Decimal values
use std::sync::LazyLock;

pub static UNITS: LazyLock<Vec<RuntimeUnitDef>> = LazyLock::new(|| {
    vec![
        // === Length (base: meter) ===
        RuntimeUnitDef::new(
            d("1000"),
            Dimensions::length(1),
            "km",
            &["km", "kilometer", "kilometers"],
        ),
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::length(1),
            "m",
            &["m", "meter", "meters"],
        ),
        RuntimeUnitDef::new(
            d("0.01"),
            Dimensions::length(1),
            "cm",
            &["cm", "centimeter", "centimeters"],
        ),
        RuntimeUnitDef::new(
            d("0.001"),
            Dimensions::length(1),
            "mm",
            &["mm", "millimeter", "millimeters"],
        ),
        RuntimeUnitDef::new(
            d("1609.344"),
            Dimensions::length(1),
            "mi",
            &["mi", "mile", "miles"],
        ),
        RuntimeUnitDef::new(
            d("0.3048"),
            Dimensions::length(1),
            "ft",
            &["ft", "foot", "feet"],
        ),
        RuntimeUnitDef::new(
            d("0.0254"),
            Dimensions::length(1),
            "in",
            &["in", "inch", "inches"],
        ),
        RuntimeUnitDef::new(
            d("0.9144"),
            Dimensions::length(1),
            "yd",
            &["yd", "yard", "yards"],
        ),
        RuntimeUnitDef::new(
            d("1852"),
            Dimensions::length(1),
            "nmi",
            &["nmi", "nautical mile", "nautical miles"],
        ),
        // === Area (base: m²) ===
        RuntimeUnitDef::new(
            d("1000000"),
            Dimensions::length(2),
            "km²",
            &["km2", "km²", "sq km"],
        ),
        RuntimeUnitDef::new(d("1"), Dimensions::length(2), "m²", &["m2", "m²", "sq m"]),
        RuntimeUnitDef::new(
            d("0.0001"),
            Dimensions::length(2),
            "cm²",
            &["cm2", "cm²", "sq cm"],
        ),
        RuntimeUnitDef::new(
            d("10000"),
            Dimensions::length(2),
            "ha",
            &["ha", "hectare", "hectares"],
        ),
        RuntimeUnitDef::new(
            d("4046.8564224"),
            Dimensions::length(2),
            "acre",
            &["acre", "acres"],
        ),
        RuntimeUnitDef::new(
            d("0.09290304"),
            Dimensions::length(2),
            "ft²",
            &["ft2", "ft²", "sq ft"],
        ),
        // === Volume (base: m³, but L is more common) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::length(3),
            "m³",
            &["m3", "m³", "cubic meter"],
        ),
        RuntimeUnitDef::new(
            d("0.001"),
            Dimensions::length(3),
            "L",
            &["l", "L", "liter", "liters", "litre", "litres"],
        ),
        RuntimeUnitDef::new(
            d("0.000001"),
            Dimensions::length(3),
            "mL",
            &["ml", "mL", "milliliter", "milliliters"],
        ),
        RuntimeUnitDef::new(
            d("0.00378541"),
            Dimensions::length(3),
            "gal",
            &["gal", "gallon", "gallons"],
        ),
        RuntimeUnitDef::new(
            d("0.000946353"),
            Dimensions::length(3),
            "qt",
            &["qt", "quart", "quarts"],
        ),
        RuntimeUnitDef::new(
            d("0.000473176"),
            Dimensions::length(3),
            "pt",
            &["pt", "pint", "pints"],
        ),
        RuntimeUnitDef::new(
            d("0.000236588"),
            Dimensions::length(3),
            "cup",
            &["cup", "cups"],
        ),
        RuntimeUnitDef::new(
            d("0.0000295735"),
            Dimensions::length(3),
            "fl oz",
            &["fl oz", "floz", "fluid ounce"],
        ),
        // === Mass (base: kilogram) ===
        RuntimeUnitDef::new(
            d("1000"),
            Dimensions::mass(1),
            "t",
            &["t", "ton", "tons", "tonne", "tonnes"],
        ),
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::mass(1),
            "kg",
            &["kg", "kilogram", "kilograms"],
        ),
        RuntimeUnitDef::new(
            d("0.001"),
            Dimensions::mass(1),
            "g",
            &["g", "gram", "grams"],
        ),
        RuntimeUnitDef::new(
            d("0.000001"),
            Dimensions::mass(1),
            "mg",
            &["mg", "milligram", "milligrams"],
        ),
        RuntimeUnitDef::new(
            d("0.45359237"),
            Dimensions::mass(1),
            "lb",
            &["lb", "lbs", "pound", "pounds"],
        ),
        RuntimeUnitDef::new(
            d("0.0283495"),
            Dimensions::mass(1),
            "oz",
            &["oz", "ounce", "ounces"],
        ),
        // === Time (base: second) ===
        RuntimeUnitDef::new(
            d("31557600"),
            Dimensions::time(1),
            "yr",
            &["yr", "year", "years"],
        ),
        RuntimeUnitDef::new(
            d("2629746"),
            Dimensions::time(1),
            "mo",
            &["mo", "month", "months"],
        ),
        RuntimeUnitDef::new(
            d("604800"),
            Dimensions::time(1),
            "wk",
            &["wk", "week", "weeks"],
        ),
        RuntimeUnitDef::new(d("86400"), Dimensions::time(1), "d", &["d", "day", "days"]),
        RuntimeUnitDef::new(
            d("3600"),
            Dimensions::time(1),
            "h",
            &["h", "hr", "hour", "hours"],
        ),
        RuntimeUnitDef::new(
            d("60"),
            Dimensions::time(1),
            "min",
            &["min", "minute", "minutes"],
        ),
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::time(1),
            "s",
            &["s", "sec", "second", "seconds"],
        ),
        RuntimeUnitDef::new(
            d("0.001"),
            Dimensions::time(1),
            "ms",
            &["ms", "millisecond", "milliseconds"],
        ),
        // === Speed (base: m/s) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                length: 1,
                time: -1,
                ..Dimensions::ZERO
            },
            "m/s",
            &["m/s", "mps"],
        ),
        RuntimeUnitDef::new(
            d("0.277778"),
            Dimensions {
                length: 1,
                time: -1,
                ..Dimensions::ZERO
            },
            "km/h",
            &["km/h", "kph", "kmh"],
        ),
        RuntimeUnitDef::new(
            d("0.44704"),
            Dimensions {
                length: 1,
                time: -1,
                ..Dimensions::ZERO
            },
            "mph",
            &["mph"],
        ),
        RuntimeUnitDef::new(
            d("0.514444"),
            Dimensions {
                length: 1,
                time: -1,
                ..Dimensions::ZERO
            },
            "knot",
            &["knot", "knots", "kn"],
        ),
        // === Plane angle (base: radian) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::angle(1),
            "rad",
            &["rad", "radian", "radians"],
        ),
        RuntimeUnitDef::new(
            d("0.0174532925199432957692369077"),
            Dimensions::angle(1),
            "°",
            &["°", "deg", "degree", "degrees"],
        ),
        // === Temperature (base: Kelvin, but we use Celsius as practical base) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::temperature(1),
            "°C",
            &["c", "C", "celsius", "°c", "°C"],
        ),
        // F to C: C = (F - 32) * 5/9, so factor = 5/9, offset = -32
        RuntimeUnitDef::with_offset(
            Decimal::new(5, 0) / Decimal::new(9, 0),
            d("-32"),
            Dimensions::temperature(1),
            "°F",
            &["f", "F", "fahrenheit", "°f", "°F"],
        ),
        RuntimeUnitDef::with_offset(
            d("1"),
            d("-273.15"),
            Dimensions::temperature(1),
            "K",
            &["k", "K", "kelvin"],
        ),
        // === Data (base: byte) ===
        RuntimeUnitDef::new(
            d("1099511627776"),
            Dimensions::data(1),
            "TB",
            &["tb", "TB", "terabyte", "terabytes"],
        ),
        RuntimeUnitDef::new(
            d("1073741824"),
            Dimensions::data(1),
            "GB",
            &["gb", "GB", "gigabyte", "gigabytes"],
        ),
        RuntimeUnitDef::new(
            d("1048576"),
            Dimensions::data(1),
            "MB",
            &["mb", "MB", "megabyte", "megabytes"],
        ),
        RuntimeUnitDef::new(
            d("1024"),
            Dimensions::data(1),
            "KB",
            &["kb", "KB", "kilobyte", "kilobytes"],
        ),
        RuntimeUnitDef::new(
            d("1"),
            Dimensions::data(1),
            "B",
            &["b", "B", "byte", "bytes"],
        ),
        RuntimeUnitDef::new(d("0.125"), Dimensions::data(1), "bit", &["bit", "bits"]),
        // === Force (base: Newton = kg·m/s²) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                mass: 1,
                length: 1,
                time: -2,
                ..Dimensions::ZERO
            },
            "N",
            &["n", "N", "newton", "newtons"],
        ),
        RuntimeUnitDef::new(
            d("4.44822"),
            Dimensions {
                mass: 1,
                length: 1,
                time: -2,
                ..Dimensions::ZERO
            },
            "lbf",
            &["lbf", "pound-force"],
        ),
        // === Energy (base: Joule = kg·m²/s²) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "J",
            &["j", "J", "joule", "joules"],
        ),
        RuntimeUnitDef::new(
            d("1000"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "kJ",
            &["kj", "kJ", "kilojoule", "kilojoules"],
        ),
        RuntimeUnitDef::new(
            d("4.184"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "cal",
            &["cal", "calorie", "calories"],
        ),
        RuntimeUnitDef::new(
            d("4184"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "kcal",
            &["kcal", "kilocalorie", "kilocalories"],
        ),
        RuntimeUnitDef::new(
            d("3600000"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "kWh",
            &["kwh", "kWh", "kilowatt-hour"],
        ),
        RuntimeUnitDef::new(
            d("3600"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -2,
                ..Dimensions::ZERO
            },
            "Wh",
            &["wh", "Wh", "watt-hour"],
        ),
        // === Power (base: Watt = kg·m²/s³) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -3,
                ..Dimensions::ZERO
            },
            "W",
            &["w", "W", "watt", "watts"],
        ),
        RuntimeUnitDef::new(
            d("1000"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -3,
                ..Dimensions::ZERO
            },
            "kW",
            &["kw", "kW", "kilowatt", "kilowatts"],
        ),
        RuntimeUnitDef::new(
            d("1000000"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -3,
                ..Dimensions::ZERO
            },
            "MW",
            &["mw", "MW", "megawatt", "megawatts"],
        ),
        RuntimeUnitDef::new(
            d("745.7"),
            Dimensions {
                mass: 1,
                length: 2,
                time: -3,
                ..Dimensions::ZERO
            },
            "hp",
            &["hp", "horsepower"],
        ),
        // === Pressure (base: Pascal = kg/(m·s²)) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                mass: 1,
                length: -1,
                time: -2,
                ..Dimensions::ZERO
            },
            "Pa",
            &["pa", "Pa", "pascal", "pascals"],
        ),
        RuntimeUnitDef::new(
            d("1000"),
            Dimensions {
                mass: 1,
                length: -1,
                time: -2,
                ..Dimensions::ZERO
            },
            "kPa",
            &["kpa", "kPa", "kilopascal"],
        ),
        RuntimeUnitDef::new(
            d("100000"),
            Dimensions {
                mass: 1,
                length: -1,
                time: -2,
                ..Dimensions::ZERO
            },
            "bar",
            &["bar"],
        ),
        RuntimeUnitDef::new(
            d("6894.76"),
            Dimensions {
                mass: 1,
                length: -1,
                time: -2,
                ..Dimensions::ZERO
            },
            "psi",
            &["psi"],
        ),
        RuntimeUnitDef::new(
            d("101325"),
            Dimensions {
                mass: 1,
                length: -1,
                time: -2,
                ..Dimensions::ZERO
            },
            "atm",
            &["atm", "atmosphere"],
        ),
        // === Acceleration (base: m/s²) ===
        RuntimeUnitDef::new(
            d("1"),
            Dimensions {
                length: 1,
                time: -2,
                ..Dimensions::ZERO
            },
            "m/s²",
            &["m/s2", "m/s²", "mps2"],
        ),
    ]
});

/// Runtime unit definition with proper Decimal values
pub struct RuntimeUnitDef {
    pub factor: Decimal,
    pub offset: Decimal,
    pub dimensions: Dimensions,
    pub symbol: &'static str,
    pub aliases: &'static [&'static str],
}

impl RuntimeUnitDef {
    fn new(
        factor: Decimal,
        dimensions: Dimensions,
        symbol: &'static str,
        aliases: &'static [&'static str],
    ) -> Self {
        Self {
            factor,
            offset: Decimal::ZERO,
            dimensions,
            symbol,
            aliases,
        }
    }

    fn with_offset(
        factor: Decimal,
        offset: Decimal,
        dimensions: Dimensions,
        symbol: &'static str,
        aliases: &'static [&'static str],
    ) -> Self {
        Self {
            factor,
            offset,
            dimensions,
            symbol,
            aliases,
        }
    }

    pub fn to_compound_unit(&self) -> CompoundUnit {
        CompoundUnit {
            factor: self.factor,
            offset: self.offset,
            dimensions: self.dimensions,
            symbol: self.symbol.to_string(),
        }
    }
}

// ============================================================================
// PARSING & LOOKUP
// ============================================================================

/// Parse a unit string into a CompoundUnit
pub fn parse_unit(s: &str) -> Option<CompoundUnit> {
    let lower = s.to_lowercase();
    UNITS
        .iter()
        .find(|def| {
            def.symbol.eq_ignore_ascii_case(s)
                || def.aliases.iter().any(|a| a.to_lowercase() == lower)
        })
        .map(|def| def.to_compound_unit())
}

/// Get all unit aliases (for syntax highlighting)
pub fn all_aliases() -> impl Iterator<Item = &'static str> {
    UNITS.iter().flat_map(|d| d.aliases.iter().copied())
}

/// Get all unit symbols (for syntax highlighting)
pub fn all_symbols() -> impl Iterator<Item = &'static str> {
    UNITS.iter().map(|d| d.symbol)
}

pub fn try_convert(
    value: Decimal,
    from: &CompoundUnit,
    to: &CompoundUnit,
) -> Result<Option<Decimal>, EvalError> {
    from.try_convert_to(value, to)
}

// ============================================================================
// FORMATTING HELPERS
// ============================================================================

/// Format an exponent as superscript
fn format_exponent(exp: i8) -> String {
    if exp == 1 {
        return String::new();
    }
    let superscripts = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut result = String::new();
    let abs_exp = exp.unsigned_abs() as usize;
    if exp < 0 {
        result.push('⁻');
    }
    if abs_exp < 10 {
        result.push(superscripts[abs_exp]);
    } else {
        for digit in abs_exp.to_string().chars() {
            let d = digit.to_digit(10).unwrap() as usize;
            result.push(superscripts[d]);
        }
    }
    result
}

/// Generate a smart symbol for compound units
/// Tries to find a matching unit in the registry, otherwise builds from dimensions
fn smart_symbol(
    left: &str,
    right: &str,
    dims: &Dimensions,
    factor: Decimal,
    multiply: bool,
) -> String {
    // Check for dimensionless
    if dims.is_dimensionless() {
        return String::new();
    }

    // Look for a matching unit in the registry
    if let Some(matching) = find_unit_by_dimensions_and_factor(dims, factor) {
        return matching.to_string();
    }

    // Fall back to building the symbol
    format_compound_symbol(left, right, multiply)
}

/// Find a unit in the registry that matches the dimensions and factor
fn find_unit_by_dimensions_and_factor(dims: &Dimensions, factor: Decimal) -> Option<&'static str> {
    // Check lazy-initialized UNITS registry
    for def in UNITS.iter() {
        if def.dimensions == *dims {
            // Factor match with some tolerance for floating point
            let ratio = if factor.is_zero() || def.factor.is_zero() {
                if factor == def.factor {
                    Decimal::ONE
                } else {
                    Decimal::ZERO
                }
            } else {
                let Some(ratio) = factor.checked_div(def.factor) else {
                    continue;
                };
                ratio
            };
            // Allow 0.1% tolerance for floating point conversion factors
            if (ratio - Decimal::ONE).abs() < d(CONVERSION_TOLERANCE) {
                return Some(def.symbol);
            }
        }
    }
    None
}

/// Format a compound unit symbol from two units
fn format_compound_symbol(left: &str, right: &str, multiply: bool) -> String {
    if multiply {
        // If same unit, use exponent notation (m * m = m²)
        if left == right {
            format!("{}{}", left, format_exponent(2))
        } else {
            // For multiplication of different units, use ·
            format!("{}·{}", left, right)
        }
    } else {
        // For division, use /
        format!("{}/{}", left, right)
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimensions_multiply() {
        let length = Dimensions::length(1);
        let area = length.checked_multiply(length).unwrap();
        assert_eq!(area.length, 2);
    }

    #[test]
    fn test_dimensions_divide() {
        let length = Dimensions::length(1);
        let time = Dimensions::time(1);
        let speed = length.checked_divide(time).unwrap();
        assert_eq!(speed.length, 1);
        assert_eq!(speed.time, -1);
    }

    #[test]
    fn test_parse_unit() {
        let meter = parse_unit("m").unwrap();
        assert_eq!(meter.factor, d("1"));
        assert_eq!(meter.dimensions, Dimensions::length(1));

        let km = parse_unit("km").unwrap();
        assert_eq!(km.factor, d("1000"));
    }

    #[test]
    fn test_unit_conversion() {
        let km = parse_unit("km").unwrap();
        let m = parse_unit("m").unwrap();
        let result = km.try_convert_to(d("1"), &m).unwrap().unwrap();
        assert_eq!(result, d("1000"));
    }

    #[test]
    fn test_compound_multiply() {
        let m = parse_unit("m").unwrap();
        let m2 = m.try_multiply(&m).unwrap();
        assert_eq!(m2.dimensions.length, 2);
    }

    #[test]
    fn test_compound_divide() {
        let km = parse_unit("km").unwrap();
        let h = parse_unit("h").unwrap();
        let kmh = km.try_divide(&h).unwrap();
        assert_eq!(kmh.dimensions.length, 1);
        assert_eq!(kmh.dimensions.time, -1);
    }

    #[test]
    fn test_format_exponent() {
        assert_eq!(format_exponent(2), "²");
        assert_eq!(format_exponent(3), "³");
        assert_eq!(format_exponent(-1), "⁻¹");
        assert_eq!(format_exponent(1), "");
    }
}
