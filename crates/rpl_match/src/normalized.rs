use std::hash::Hash;

use rpl_constraints::attributes::ExtraSpan;
use rpl_context::pat::MatchedMap;
use rustc_hir::def_id::LocalDefId;

use crate::mir::pat;

pub trait NormalizedMatched<'tcx>: Eq + Hash + Sized {
    type Matched;

    /// Create a new [`NormalizedMatched`] from a [`Matched`] and a [`pat::LabelMap`].
    fn new(
        bottom: LocalDefId,
        matched: &Self::Matched,
        label_map: &pat::LabelMap,
        extra_spans: &ExtraSpan<'tcx>,
    ) -> Self;

    // /// Map [`Matched`] from one pattern to another.
    // ///
    // /// This is useful for normalizing patterns that have been matched against a different set of
    // /// meta variables.
    // fn normalize(matched_map: &MatchedMap, matched_from: &Self::Matched, label_map_from:
    // &pat::LabelMap) -> Self {     Self::map(
    //         Self::new(bottom, matched_from, label_map_from, extra_spans),
    //         matched_map,
    //     )
    // }

    /// Map [`Matched`] from one pattern to another.
    ///
    /// This is useful for normalizing patterns that have been matched against a different set of
    /// meta variables.
    fn map(self, matched_map: &MatchedMap) -> Self;

    fn has_same_head(&self, other: &Self) -> bool;
}
