//! Non-totalistic life-like rules.

use crate::{
    cells::{Alive, CellRef, ConflReason, Dead, SetReason, State},
    world::World,
};
use bitflags::bitflags;
use ca_rules::{ParseNtLife, ParseRuleError};

/// The neighborhood descriptor.
///
/// It is a 20-bit integer of the form `0b_abcdefgh_ijklmnop_qr_st`,
/// where:
///
/// * `0b_ai`, `0b_bj`, ..., `0b_hp` are the states of the eight neighbors,
/// * `0b_qr` is the state of the successor.
/// * `0b_st` is the state of the cell itself.
/// * `0b_10` means dead,
/// * `0b_01` means alive,
/// * `0b_00` means unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Desc(usize);

impl Desc {
    pub(crate) fn new(state: State, succ_state: State) -> Desc {
        let nbhd_state = match state {
            Dead => 0xff00,
            Alive => 0x00ff,
        };
        Desc(nbhd_state << 4 | (succ_state as usize) << 2 | state as usize)
    }
}

impl<'a> CellRef<'a> {
    pub(crate) fn update_desc(self, old_state: Option<State>, state: Option<State>) {
        let nbhd_change_num = match (state, old_state) {
            (Some(Dead), Some(Alive)) | (Some(Alive), Some(Dead)) => 0x0101,
            (Some(Dead), None) | (None, Some(Dead)) => 0x0100,
            (Some(Alive), None) | (None, Some(Alive)) => 0x0001,
            _ => 0x0000,
        };
        for (i, &neigh) in self.nbhd.iter().rev().enumerate() {
            let neigh = neigh.unwrap();
            let mut desc = neigh.desc.get();
            desc.0 ^= nbhd_change_num << i << 4;
            neigh.desc.set(desc);
        }

        let change_num = match (state, old_state) {
            (Some(Dead), Some(Alive)) | (Some(Alive), Some(Dead)) => 0b11,
            (Some(Dead), None) | (None, Some(Dead)) => 0b10,
            (Some(Alive), None) | (None, Some(Alive)) => 0b01,
            _ => 0,
        };
        if let Some(pred) = self.pred {
            let mut desc = pred.desc.get();
            desc.0 ^= change_num << 2;
            pred.desc.set(desc);
        }
        let mut desc = self.desc.get();
        desc.0 ^= change_num;
        self.desc.set(desc);
    }
}

impl<'a> SetReason<'a> {
    pub(crate) fn cells(self, cell: CellRef<'a>) -> Vec<CellRef<'a>> {
        match self {
            SetReason::Rule(cell0) => {
                let desc = cell0.desc.get();
                let mut cells = Vec::with_capacity(10);
                if desc.0 & 0b11 != 0 && cell != cell0 {
                    cells.push(cell0);
                }
                if desc.0 & 0b11 << 2 != 0 {
                    if let Some(succ) = cell0.succ {
                        if cell != succ {
                            cells.push(succ);
                        }
                    }
                }
                for i in 0..8 {
                    if desc.0 & 0x0101 << i << 4 != 0 {
                        if let Some(neigh) = cell0.nbhd[i] {
                            if cell != neigh {
                                cells.push(neigh);
                            }
                        }
                    }
                }
                cells
            }
            SetReason::Sym(sym) => vec![sym],
            _ => Vec::new(),
        }
    }
}

impl<'a> ConflReason<'a> {
    pub(crate) fn cells(self) -> Vec<CellRef<'a>> {
        match self {
            ConflReason::Rule(cell) => {
                let desc = cell.desc.get();
                let mut cells = Vec::with_capacity(10);
                if desc.0 & 0b11 != 0 {
                    cells.push(cell);
                }
                if desc.0 & 0b11 << 2 != 0 {
                    if let Some(succ) = cell.succ {
                        cells.push(succ);
                    }
                }
                for i in 0..8 {
                    if desc.0 & 0x0101 << i << 4 != 0 {
                        if let Some(neigh) = cell.nbhd[i] {
                            cells.push(neigh);
                        }
                    }
                }
                cells
            }
            ConflReason::Sym(cell, sym) => vec![cell, sym],
            _ => Vec::new(),
        }
    }
}

bitflags! {
    /// Flags to imply the state of a cell and its neighbors.
    struct ImplFlags: u32 {
        /// A conflict is detected.
        const CONFLICT = 0b_0000_0001;

        /// The successor must be alive.
        const SUCC_ALIVE = 0b_0000_0100;

        /// The successor must be dead.
        const SUCC_DEAD = 0b_0000_1000;

        /// The state of the successor is implied.
        const SUCC = Self::SUCC_ALIVE.bits | Self::SUCC_DEAD.bits;

        /// The cell itself must be alive.
        const SELF_ALIVE = 0b_0001_0000;

        /// The cell itself must be dead.
        const SELF_DEAD = 0b_0010_0000;

        /// The state of the cell itself is implied.
        const SELF = Self::SELF_ALIVE.bits | Self::SELF_DEAD.bits;

        /// The state of at least one unknown neighbor is implied.
        const NBHD = 0xffff << 6;
    }
}

/// Non-totalistic life-like rules.
///
/// This includes any rule that can be converted to a non-totalistic
/// life-like rule: isotropic non-totalistic rules,
/// non-isotropic rules, hexagonal rules, rules with von Neumann
/// neighborhoods, etc.
pub struct Rule {
    /// Whether the rule contains `B0`.
    pub(crate) b0: bool,

    /// An array of actions for all neighborhood descriptors.
    impl_table: Vec<ImplFlags>,
}

impl Rule {
    /// Constructs a new rule from the `b` and `s` data.
    pub fn new(b: Vec<u8>, s: Vec<u8>) -> Self {
        let b0 = b.contains(&0);

        let impl_table = vec![ImplFlags::empty(); 1 << 20];

        Rule { b0, impl_table }
            .init_trans(b, s)
            .init_conflict()
            .init_impl()
            .init_impl_nbhd()
    }

    /// Deduces the implication for the successor.
    fn init_trans(mut self, b: Vec<u8>, s: Vec<u8>) -> Self {
        // Fills in the positions of the neighborhood descriptors
        // that have no unknown neighbors.
        for alives in 0..=0xff {
            let desc = (0xff & !alives) << 12 | alives << 4;
            let alives = alives as u8;
            self.impl_table[desc | Dead as usize] |= if b.contains(&alives) {
                ImplFlags::SUCC_ALIVE
            } else {
                ImplFlags::SUCC_DEAD
            };
            self.impl_table[desc | Alive as usize] |= if s.contains(&alives) {
                ImplFlags::SUCC_ALIVE
            } else {
                ImplFlags::SUCC_DEAD
            };
            self.impl_table[desc] |= if b.contains(&alives) && s.contains(&alives) {
                ImplFlags::SUCC_ALIVE
            } else if !b.contains(&alives) && !s.contains(&alives) {
                ImplFlags::SUCC_DEAD
            } else {
                ImplFlags::empty()
            };
        }

        // Fills in the other positions.
        for unknowns in 1usize..=0xff {
            // `n` is the largest power of two smaller than `unknowns`.
            let n = unknowns.next_power_of_two() >> usize::from(!unknowns.is_power_of_two());
            for alives in (0..=0xff).filter(|a| a & unknowns == 0) {
                let desc = (0xff & !alives & !unknowns) << 12 | alives << 4;
                let desc0 = (0xff & !alives & !unknowns | n) << 12 | alives << 4;
                let desc1 = (0xff & !alives & !unknowns) << 12 | (alives | n) << 4;

                for &state in [Dead as usize, Alive as usize, 0].iter() {
                    let trans0 = self.impl_table[desc0 | state];

                    if trans0 == self.impl_table[desc1 | state] {
                        self.impl_table[desc | state] |= trans0;
                    }
                }
            }
        }

        self
    }

    /// Deduces the conflicts.
    fn init_conflict(mut self) -> Self {
        for nbhd_state in 0..0xffff {
            for &state in [Dead as usize, Alive as usize, 0].iter() {
                let desc = nbhd_state << 4 | state;

                if self.impl_table[desc].contains(ImplFlags::SUCC_ALIVE) {
                    self.impl_table[desc | (Dead as usize) << 2] = ImplFlags::CONFLICT;
                } else if self.impl_table[desc].contains(ImplFlags::SUCC_DEAD) {
                    self.impl_table[desc | (Alive as usize) << 2] = ImplFlags::CONFLICT;
                }
            }
        }
        self
    }

    /// Deduces the implication for the cell itself.
    fn init_impl(mut self) -> Self {
        for unknowns in 0..=0xff {
            for alives in (0..=0xff).filter(|a| a & unknowns == 0) {
                let desc = (0xff & !alives & !unknowns) << 12 | alives << 4;

                for &succ_state in [Dead as usize, Alive as usize].iter() {
                    let flag = if succ_state == Dead as usize {
                        ImplFlags::SUCC_ALIVE | ImplFlags::CONFLICT
                    } else {
                        ImplFlags::SUCC_DEAD | ImplFlags::CONFLICT
                    };

                    let possibly_dead = !self.impl_table[desc | Dead as usize].intersects(flag);
                    let possibly_alive = !self.impl_table[desc | Alive as usize].intersects(flag);

                    let index = desc | succ_state << 2;
                    if possibly_dead && !possibly_alive {
                        self.impl_table[index] |= ImplFlags::SELF_DEAD;
                    } else if !possibly_dead && possibly_alive {
                        self.impl_table[index] |= ImplFlags::SELF_ALIVE;
                    } else if !possibly_dead && !possibly_alive {
                        self.impl_table[index] = ImplFlags::CONFLICT;
                    }
                }
            }
        }

        self
    }

    ///  Deduces the implication for the neighbors.
    fn init_impl_nbhd(mut self) -> Self {
        for unknowns in 1usize..=0xff {
            // `n` runs through all the non-zero binary digits of `unknowns`.
            for n in (0..8).map(|i| 1 << i).filter(|n| unknowns & n != 0) {
                for alives in 0..=0xff {
                    let desc = (0xff & !alives & !unknowns) << 12 | alives << 4;
                    let desc0 = (0xff & !alives & !unknowns | n) << 12 | alives << 4;
                    let desc1 = (0xff & !alives & !unknowns) << 12 | (alives | n) << 4;

                    for &succ_state in [Dead as usize, Alive as usize].iter() {
                        let flag = if succ_state == Dead as usize {
                            ImplFlags::SUCC_ALIVE | ImplFlags::CONFLICT
                        } else {
                            ImplFlags::SUCC_DEAD | ImplFlags::CONFLICT
                        };

                        let index = desc | succ_state << 2;

                        for &state in [Dead as usize, Alive as usize, 0].iter() {
                            let possibly_dead = !self.impl_table[desc0 | state].intersects(flag);
                            let possibly_alive = !self.impl_table[desc1 | state].intersects(flag);

                            if possibly_dead && !possibly_alive {
                                self.impl_table[index | state] |=
                                    ImplFlags::from_bits((n.pow(2) << 7) as u32).unwrap();
                            } else if !possibly_dead && possibly_alive {
                                self.impl_table[index | state] |=
                                    ImplFlags::from_bits((n.pow(2) << 6) as u32).unwrap();
                            } else if !possibly_dead && !possibly_alive {
                                self.impl_table[index | state] = ImplFlags::CONFLICT;
                            }
                        }
                    }
                }
            }
        }

        self
    }

    pub fn parse_rule(input: &str) -> Result<Self, ParseRuleError> {
        ParseNtLife::parse_rule(input)
    }

    pub(crate) fn consistify<'a>(
        world: &mut World<'a>,
        cell: CellRef<'a>,
    ) -> Result<(), ConflReason<'a>> {
        let flags = world.rule.impl_table[cell.desc.get().0];

        if flags.contains(ImplFlags::CONFLICT) {
            return Err(ConflReason::Rule(cell));
        }

        if flags.intersects(ImplFlags::SUCC_DEAD | ImplFlags::SUCC_ALIVE) {
            let state = if flags.contains(ImplFlags::SUCC_DEAD) {
                Dead
            } else {
                Alive
            };
            let succ = cell.succ.unwrap();
            return world.set_cell(succ, state, SetReason::Rule(cell));
        }

        if flags.intersects(ImplFlags::SELF_DEAD | ImplFlags::SELF_ALIVE) {
            let state = if flags.contains(ImplFlags::SELF_DEAD) {
                Dead
            } else {
                Alive
            };
            world.set_cell(cell, state, SetReason::Rule(cell))?;
        }

        if flags.intersects(ImplFlags::NBHD) {
            for (i, &neigh) in cell.nbhd.iter().enumerate() {
                if flags.intersects(ImplFlags::from_bits(3 << (2 * i + 6)).unwrap()) {
                    if let Some(neigh) = neigh {
                        let state =
                            if flags.contains(ImplFlags::from_bits(1 << (2 * i + 7)).unwrap()) {
                                Dead
                            } else {
                                Alive
                            };
                        world.set_cell(neigh, state, SetReason::Rule(cell))?;
                    }
                }
            }
        }

        Ok(())
    }
}

/// A parser for the rule.
impl ParseNtLife for Rule {
    fn from_bs(b: Vec<u8>, s: Vec<u8>) -> Self {
        Self::new(b, s)
    }
}
