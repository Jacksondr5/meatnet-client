use std::fmt;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductType {
    Unknown,
    PredictiveProbe,
    MeatNetRepeater,
    GiantGrillGauge,
    Display,
    Booster,
}

impl ProductType {
    pub fn from_byte(raw: u8) -> Self {
        match raw {
            1 => Self::PredictiveProbe,
            2 => Self::MeatNetRepeater,
            3 => Self::GiantGrillGauge,
            4 => Self::Display,
            5 => Self::Booster,
            _ => Self::Unknown,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::PredictiveProbe => "predictive-probe",
            Self::MeatNetRepeater => "meatnet-repeater",
            Self::GiantGrillGauge => "giant-grill-gauge",
            Self::Display => "display",
            Self::Booster => "booster",
        }
    }

    pub fn is_probe(self) -> bool {
        matches!(self, Self::PredictiveProbe)
    }

    pub fn is_node(self) -> bool {
        matches!(self, Self::MeatNetRepeater | Self::Display | Self::Booster)
    }
}

impl fmt::Display for ProductType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}
