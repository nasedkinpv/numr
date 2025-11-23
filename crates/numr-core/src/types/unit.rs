//! Physical units and conversions
//!
//! To add a new unit, simply add an entry to the UNITS array.
//! All parsing, display, and highlighting will automatically pick it up.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Categories of units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitType {
    Length,
    Weight,
    Time,
    Data,
}

/// Unit metadata - single source of truth for each unit
pub struct UnitDef {
    /// The unit enum variant
    pub unit: Unit,
    /// Unit category
    pub unit_type: UnitType,
    /// Short display name (e.g., "km", "lb")
    pub short_name: &'static str,
    /// Conversion factor to base unit of its type
    pub to_base_factor: f64,
    /// All accepted aliases for parsing (lowercase)
    pub aliases: &'static [&'static str],
}

/// Complete registry of all supported units.
/// To add a new unit: add enum variant and add entry here.
pub static UNITS: &[UnitDef] = &[
    // Length (base: meter)
    UnitDef {
        unit: Unit::Kilometer,
        unit_type: UnitType::Length,
        short_name: "km",
        to_base_factor: 1000.0,
        aliases: &["km"],
    },
    UnitDef {
        unit: Unit::Meter,
        unit_type: UnitType::Length,
        short_name: "m",
        to_base_factor: 1.0,
        aliases: &["m"],
    },
    UnitDef {
        unit: Unit::Centimeter,
        unit_type: UnitType::Length,
        short_name: "cm",
        to_base_factor: 0.01,
        aliases: &["cm"],
    },
    UnitDef {
        unit: Unit::Millimeter,
        unit_type: UnitType::Length,
        short_name: "mm",
        to_base_factor: 0.001,
        aliases: &["mm"],
    },
    UnitDef {
        unit: Unit::Mile,
        unit_type: UnitType::Length,
        short_name: "mi",
        to_base_factor: 1609.344,
        aliases: &["mi", "miles", "mile"],
    },
    UnitDef {
        unit: Unit::Foot,
        unit_type: UnitType::Length,
        short_name: "ft",
        to_base_factor: 0.3048,
        aliases: &["ft", "feet", "foot"],
    },
    UnitDef {
        unit: Unit::Inch,
        unit_type: UnitType::Length,
        short_name: "in",
        to_base_factor: 0.0254,
        aliases: &["inches", "inch"],
    },
    // Weight (base: gram)
    UnitDef {
        unit: Unit::Kilogram,
        unit_type: UnitType::Weight,
        short_name: "kg",
        to_base_factor: 1000.0,
        aliases: &["kg"],
    },
    UnitDef {
        unit: Unit::Gram,
        unit_type: UnitType::Weight,
        short_name: "g",
        to_base_factor: 1.0,
        aliases: &["g"],
    },
    UnitDef {
        unit: Unit::Milligram,
        unit_type: UnitType::Weight,
        short_name: "mg",
        to_base_factor: 0.001,
        aliases: &["mg"],
    },
    UnitDef {
        unit: Unit::Pound,
        unit_type: UnitType::Weight,
        short_name: "lb",
        to_base_factor: 453.592,
        aliases: &["lb", "lbs", "pound", "pounds"],
    },
    UnitDef {
        unit: Unit::Ounce,
        unit_type: UnitType::Weight,
        short_name: "oz",
        to_base_factor: 28.3495,
        aliases: &["oz", "ounce", "ounces"],
    },
    // Time (base: second)
    UnitDef {
        unit: Unit::Week,
        unit_type: UnitType::Time,
        short_name: "wk",
        to_base_factor: 604800.0,
        aliases: &["wk", "week", "weeks"],
    },
    UnitDef {
        unit: Unit::Day,
        unit_type: UnitType::Time,
        short_name: "d",
        to_base_factor: 86400.0,
        aliases: &["d", "day", "days"],
    },
    UnitDef {
        unit: Unit::Hour,
        unit_type: UnitType::Time,
        short_name: "h",
        to_base_factor: 3600.0,
        aliases: &["h", "hr", "hour", "hours"],
    },
    UnitDef {
        unit: Unit::Minute,
        unit_type: UnitType::Time,
        short_name: "min",
        to_base_factor: 60.0,
        aliases: &["min", "minute", "minutes"],
    },
    UnitDef {
        unit: Unit::Second,
        unit_type: UnitType::Time,
        short_name: "s",
        to_base_factor: 1.0,
        aliases: &["s", "sec", "second", "seconds"],
    },
    // Data (base: byte)
    UnitDef {
        unit: Unit::Terabyte,
        unit_type: UnitType::Data,
        short_name: "TB",
        to_base_factor: 1_099_511_627_776.0,
        aliases: &["tb"],
    },
    UnitDef {
        unit: Unit::Gigabyte,
        unit_type: UnitType::Data,
        short_name: "GB",
        to_base_factor: 1_073_741_824.0,
        aliases: &["gb"],
    },
    UnitDef {
        unit: Unit::Megabyte,
        unit_type: UnitType::Data,
        short_name: "MB",
        to_base_factor: 1_048_576.0,
        aliases: &["mb"],
    },
    UnitDef {
        unit: Unit::Kilobyte,
        unit_type: UnitType::Data,
        short_name: "KB",
        to_base_factor: 1024.0,
        aliases: &["kb"],
    },
    UnitDef {
        unit: Unit::Byte,
        unit_type: UnitType::Data,
        short_name: "B",
        to_base_factor: 1.0,
        aliases: &["b", "bytes", "byte"],
    },
];

/// Supported units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Unit {
    // Length
    Kilometer,
    Meter,
    Centimeter,
    Millimeter,
    Mile,
    Foot,
    Inch,
    // Weight
    Kilogram,
    Gram,
    Milligram,
    Pound,
    Ounce,
    // Time
    Day,
    Week,
    Hour,
    Minute,
    Second,
    // Data
    Terabyte,
    Gigabyte,
    Megabyte,
    Kilobyte,
    Byte,
}

impl Unit {
    /// Get the unit definition
    pub fn def(&self) -> &'static UnitDef {
        UNITS
            .iter()
            .find(|d| d.unit == *self)
            .expect("All units must have definitions")
    }

    /// Get the unit type/category
    pub fn unit_type(&self) -> UnitType {
        self.def().unit_type
    }

    /// Get the base unit for this unit type
    pub fn base_unit(&self) -> Unit {
        match self.unit_type() {
            UnitType::Length => Unit::Meter,
            UnitType::Weight => Unit::Gram,
            UnitType::Time => Unit::Second,
            UnitType::Data => Unit::Byte,
        }
    }

    /// Conversion factor to base unit
    pub fn to_base_factor(&self) -> f64 {
        self.def().to_base_factor
    }

    /// Get short display name
    pub fn short_name(&self) -> &'static str {
        self.def().short_name
    }

    /// Get all unit aliases (for UI highlighting)
    pub fn all_aliases() -> impl Iterator<Item = &'static str> {
        UNITS.iter().flat_map(|d| d.aliases.iter().copied())
    }

    /// Get all short names (for UI highlighting)
    pub fn all_short_names() -> impl Iterator<Item = &'static str> {
        UNITS.iter().map(|d| d.short_name)
    }

    /// Parse unit from string
    pub fn parse(s: &str) -> Option<Unit> {
        let lower = s.to_lowercase();
        UNITS
            .iter()
            .find(|d| d.short_name.eq_ignore_ascii_case(s) || d.aliases.iter().any(|a| *a == lower))
            .map(|d| d.unit)
    }

    /// Iterator over all units
    pub fn all() -> impl Iterator<Item = Unit> {
        UNITS.iter().map(|d| d.unit)
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short_name())
    }
}

/// Convert a value from one unit to another
pub fn convert(value: f64, from: Unit, to: Unit) -> Option<f64> {
    if from.unit_type() != to.unit_type() {
        return None; // Can't convert between different unit types
    }

    // Convert to base unit, then to target unit
    let base_value = value * from.to_base_factor();
    Some(base_value / to.to_base_factor())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_conversion() {
        let km_to_m = convert(1.0, Unit::Kilometer, Unit::Meter);
        assert_eq!(km_to_m, Some(1000.0));

        let mi_to_km = convert(1.0, Unit::Mile, Unit::Kilometer);
        assert!((mi_to_km.unwrap() - 1.609344).abs() < 0.0001);
    }

    #[test]
    fn test_time_conversion() {
        let hours_to_min = convert(2.0, Unit::Hour, Unit::Minute);
        assert_eq!(hours_to_min, Some(120.0));
    }

    #[test]
    fn test_incompatible_units() {
        let result = convert(1.0, Unit::Kilometer, Unit::Kilogram);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_units() {
        assert_eq!(Unit::parse("km"), Some(Unit::Kilometer));
        assert_eq!(Unit::parse("miles"), Some(Unit::Mile));
        assert_eq!(Unit::parse("hours"), Some(Unit::Hour));
        assert_eq!(Unit::parse("GB"), Some(Unit::Gigabyte));
    }

    #[test]
    fn test_all_units_have_defs() {
        for unit in Unit::all() {
            let def = unit.def();
            assert!(!def.short_name.is_empty());
            assert!(!def.aliases.is_empty());
        }
    }
}
