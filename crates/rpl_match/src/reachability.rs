use std::cmp::Ordering;
use std::ops::Index;

use rpl_context::pat;
use rustc_index::{Idx, IndexVec};
use rustc_middle::mir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reachable {
    None,
    From,
    To,
    Both,
}

impl Reachable {
    const fn as_ref<'a>(self) -> &'a Self {
        match self {
            Reachable::None => &Reachable::None,
            Reachable::From => &Reachable::From,
            Reachable::To => &Reachable::To,
            Reachable::Both => &Reachable::Both,
        }
    }
    pub const fn covered_by(&self, other: &Self) -> bool {
        match (self, other) {
            (Reachable::None, _) => true,
            (Reachable::From, Reachable::From) | (Reachable::From, Reachable::Both) => true,
            (Reachable::To, Reachable::To) | (Reachable::To, Reachable::Both) => true,
            (Reachable::Both, Reachable::Both) => true,
            _ => false,
        }
    }
}

pub struct Reachability<BB: Idx> {
    inner: IndexVec<BB, IndexVec<BB, bool>>,
}

impl Reachability<mir::BasicBlock> {
    pub(crate) fn is_reachable(&self, a: mir::Location, b: mir::Location) -> Reachable {
        if a.block == b.block {
            match a.statement_index.cmp(&b.statement_index) {
                Ordering::Less => return Reachable::From,
                Ordering::Greater => return Reachable::To,
                Ordering::Equal => return Reachable::Both,
            }
        }
        let from_to = self.inner[a.block][b.block];
        let to_from = self.inner[b.block][a.block];
        match (from_to, to_from) {
            (true, true) => Reachable::Both,
            (true, false) => Reachable::From,
            (false, true) => Reachable::To,
            (false, false) => Reachable::None,
        }
    }
}
impl Reachability<pat::BasicBlock> {
    pub(crate) fn is_reachable(&self, a: pat::Location, b: pat::Location) -> Reachable {
        if a.block == b.block {
            match a.statement_index.cmp(&b.statement_index) {
                Ordering::Less => return Reachable::From,
                Ordering::Greater => return Reachable::To,
                Ordering::Equal => return Reachable::Both,
            }
        }
        let from_to = self.inner[a.block][b.block];
        let to_from = self.inner[b.block][a.block];
        match (from_to, to_from) {
            (true, true) => Reachable::Both,
            (true, false) => Reachable::From,
            (false, true) => Reachable::To,
            (false, false) => Reachable::None,
        }
    }
}

impl Index<(mir::Location, mir::Location)> for Reachability<mir::BasicBlock> {
    type Output = Reachable;

    fn index(&self, index: (mir::Location, mir::Location)) -> &Self::Output {
        let (from, to) = index;
        self.is_reachable(from, to).as_ref()
    }
}
impl Index<(pat::Location, pat::Location)> for Reachability<pat::BasicBlock> {
    type Output = Reachable;

    fn index(&self, index: (pat::Location, pat::Location)) -> &Self::Output {
        let (from, to) = index;
        self.is_reachable(from, to).as_ref()
    }
}

impl Reachability<mir::BasicBlock> {
    /// Collect the reachability information for the given MIR body using
    /// the Floyd-Warshall algorithm.
    /// The result indicates whether a basic block is reachable from another.
    /// Note that within a basic block, earlier statements are not reachable
    /// from later statements.
    pub fn new_mir<'tcx>(body: &mir::Body<'tcx>) -> Reachability<mir::BasicBlock> {
        let num_blocks = body.basic_blocks.len();
        let mut inner = IndexVec::from_fn_n(|_| IndexVec::from_fn_n(|_| false, num_blocks), num_blocks);

        for (bb_idx, bb) in body.basic_blocks.iter_enumerated() {
            for succ in bb.terminator().successors() {
                inner[bb_idx][succ] = true;
            }
        }

        for k in 0..num_blocks {
            let k = mir::BasicBlock::from_usize(k);
            for i in 0..num_blocks {
                let i = mir::BasicBlock::from_usize(i);
                for j in 0..num_blocks {
                    let j = mir::BasicBlock::from_usize(j);
                    if inner[i][k] && inner[k][j] {
                        inner[i][j] = true;
                    }
                }
            }
        }

        Reachability { inner }
    }
}
impl Reachability<pat::BasicBlock> {
    /// Collect the reachability information for the given MIR body using
    /// the Floyd-Warshall algorithm.
    /// The result indicates whether a basic block is reachable from another.
    /// Note that within a basic block, earlier statements are not reachable
    /// from later statements.
    pub fn new_pat<'tcx>(body: &pat::FnPatternBody<'tcx>) -> Reachability<pat::BasicBlock> {
        let num_blocks = body.basic_blocks.len();
        let mut inner = IndexVec::from_fn_n(|_| IndexVec::from_fn_n(|_| false, num_blocks), num_blocks);

        for (bb_idx, bb) in body.basic_blocks.iter_enumerated() {
            for succ in bb.terminator().successors() {
                inner[bb_idx][succ] = true;
            }
        }

        for k in 0..num_blocks {
            let k = pat::BasicBlock::from_usize(k);
            for i in 0..num_blocks {
                let i = pat::BasicBlock::from_usize(i);
                for j in 0..num_blocks {
                    let j = pat::BasicBlock::from_usize(j);
                    if inner[i][k] && inner[k][j] {
                        inner[i][j] = true;
                    }
                }
            }
        }

        Reachability { inner }
    }
}
