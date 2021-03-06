use crate::cells::Coord;
use ca_rules::ParseRuleError;
use thiserror::Error;

/// All kinds of errors in this crate.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Unable to get the state of cell {0:?}")]
    GetCellError(Coord),
    #[error("Unable to set cell at {0:?}")]
    SetCellError(Coord),
    #[error("Invalid rule: {0:?}")]
    ParseRuleError(#[from] ParseRuleError),
}
