//! Value types for numr calculations

pub mod currency;
pub mod unit;
mod value;

pub use currency::{Currency, CurrencyDef, CURRENCIES};
pub use unit::{Unit, UnitDef, UnitType, UNITS};
pub use value::Value;
