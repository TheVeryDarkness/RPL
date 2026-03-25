use std::hash::Hash;

use rpl_constraints::attributes::ExtraSpan;
use rpl_context::pat::MatchedMap;

use crate::mir::pat;

pub trait NormalizedMatched<'tcx>: Eq + Hash {
    type Matched;

    /// Create a new [`NormalizedMatched`] from a [`Matched`] and a [`pat::LabelMap`].
    fn new(matched: &Self::Matched, label_map: &pat::LabelMap, extra_spans: &ExtraSpan<'tcx>) -> Self;

    /// Map [`Matched`] from one pattern to another.
    ///
    /// This is useful for normalizing patterns that have been matched against a different set of
    /// meta variables.
    fn normalize(matched_map: &MatchedMap, matched_from: &Self::Matched, label_map_from: &pat::LabelMap) -> Self;

    /// Map [`Matched`] from one pattern to another.
    ///
    /// This is useful for normalizing patterns that have been matched against a different set of
    /// meta variables.
    fn map(self, matched_map: &MatchedMap) -> Self;

    fn has_same_head(&self, other: &Self) -> bool;
}
