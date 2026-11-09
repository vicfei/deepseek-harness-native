//! Signals — the coeffect side of the context paradigm: named, versioned
//! context slices whose changes are classified against component specs.

use serde_json::Value;

pub type SignalKey = String;
pub type SignalValue = Value;

/// A declarative entry in a cordis.yml-shaped list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub config: SignalValue,
}
