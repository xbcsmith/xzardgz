use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
pub enum DocCategory {
    Tutorial,    // docs/tutorials/
    HowTo,       // docs/how_to/
    Explanation, // docs/explanation/
    Reference,   // docs/reference/
}

impl DocCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocCategory::Tutorial => "tutorial",
            DocCategory::HowTo => "how-to",
            DocCategory::Explanation => "explanation",
            DocCategory::Reference => "reference",
        }
    }

    pub fn directory(&self) -> &'static str {
        match self {
            DocCategory::Tutorial => "docs/tutorials",
            DocCategory::HowTo => "docs/how_to",
            DocCategory::Explanation => "docs/explanation",
            DocCategory::Reference => "docs/reference",
        }
    }
}

impl fmt::Display for DocCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
