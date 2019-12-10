//! Cellular automata rules.
//!
//! For the notations of rule strings, please see
//! [this article on LifeWiki](https://www.conwaylife.com/wiki/Rulestring).

mod life;
mod ntlife;

use crate::{
    cells::{CellRef, State},
    world::World,
};
pub use life::Life;
pub use ntlife::NtLife;

/// A cellular automaton rule.
pub trait Rule: Sized {
    /// The type of neighborhood descriptor of the rule.
    ///
    /// It describes the states of the successor and neighbors of a cell,
    /// and is used to determine the state of the cell in the next generation.
    type Desc: Copy;

    /// Whether the rule contains `B0`.
    ///
    /// In other words, whether a cell would become `Alive` in the next
    /// generation, if all its neighbors in this generation are dead.
    fn has_b0(&self) -> bool;

    /// Generates a neighborhood descriptor which says that all neighboring
    /// cells have states `state`, and the successor has state `succ_state`.
    fn new_desc(state: State, succ_state: State) -> Self::Desc;

    /// Updates the neighborhood descriptors of all neighbors and the predecessor
    /// when the state of one cell is changed.
    fn update_desc(cell: CellRef<Self>, old_state: Option<State>, state: Option<State>);

    /// Consistifies a cell.
    ///
    /// Examines the state and the neighborhood descriptor of the cell,
    /// and makes sure that it can validly produce the cell in the next
    /// generation. If possible, determines the states of some of the
    /// cells involved.
    ///
    /// Returns `false` if there is a conflict,
    /// `true` if the cells are consistent.
    fn consistify<'a>(world: &mut World<'a, Self>, cell: CellRef<'a, Self>) -> bool;
}
