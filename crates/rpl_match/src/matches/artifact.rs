use rpl_constraints::attributes::ExtraSpan;
use rpl_context::pat::{MatchedMap, Spanned};
use rustc_data_structures::sorted_map::SortedMap;
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_middle::mir::{Body, Local, PlaceRef};
use rustc_middle::ty::Ty;
use rustc_span::{Span, Symbol};

use super::{Const, Matched, StatementMatch, pat};

/// A normalized version of [`Spanned`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NormalizedSpanned {
    Location(StatementMatch),
    Local(Local),
    Body,
    Output,
    /// A span that is not associated with a specific location, local, or body,
    /// especially cannot be retrieved from some indices.
    /// For example, [`rustc_hir::Attribute`].
    Span(Span),
}

impl NormalizedSpanned {
    pub fn span(self, body: &Body<'_>, decl: &FnDecl<'_>) -> Span {
        match self {
            Self::Location(location) => location.span_no_inline(body),
            Self::Local(local) => body.local_decls[local].source_info.span,
            // Special case for the function name, which is not a label.
            Self::Body => body.span,
            Self::Output => decl.output.span(),
            Self::Span(span) => span,
        }
    }
}

/// A normalized version of [`Matched`].
///
/// # Normalization
///
/// Normalization here means that:
///
/// - The matched meta variables and labels are mapped to a canonical form
///  based on a provided mapping (`MatchedMap`).
/// - Some information that is not relevant for equality comparison is discarded.
/// - Some extra spans are included to capture additional function context.
///
/// This makes it so that two `NormalizedMatched` can be compared for equality even if they
/// were matched against different patterns with an identical set of meta variable and label names.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct NormalizedMatched<'tcx> {
    pub ty_vars: IndexVec<pat::TyVarIdx, Ty<'tcx>>,
    pub const_vars: IndexVec<pat::ConstVarIdx, Const<'tcx>>,
    pub place_vars: IndexVec<pat::PlaceVarIdx, PlaceRef<'tcx>>,
    /// Labels and attributes. Sorted by label.
    extra: SortedMap<Symbol, NormalizedSpanned>,
}

impl<'tcx> crate::normalized::NormalizedMatched<'tcx> for NormalizedMatched<'tcx> {
    type Matched = Matched<'tcx>;

    /// Create a new [`NormalizedMatched`] from a [`Matched`] and a [`pat::LabelMap`].
    #[instrument(level = "trace", ret)]
    fn new(matched: &Self::Matched, label_map: &pat::LabelMap, extra_spans: &ExtraSpan<'tcx>) -> Self {
        let ty_vars = matched.ty_vars.clone();
        let const_vars = matched.const_vars.clone();
        let place_vars = matched.place_vars.clone();
        let labels: SortedMap<_, _> = label_map
            .iter()
            .map(|(label, spanned)| match spanned {
                Spanned::Location(location) => (*label, NormalizedSpanned::Location(matched[*location])),
                Spanned::Local(local) => (*label, NormalizedSpanned::Local(matched[*local])),
                Spanned::Body => (*label, NormalizedSpanned::Body),
                Spanned::Output => (*label, NormalizedSpanned::Output),
            })
            .chain(
                extra_spans
                    .iter()
                    .map(|(label, span)| (*label, NormalizedSpanned::Span(span.span()))),
            )
            .collect();

        Self {
            ty_vars,
            const_vars,
            place_vars,
            extra: labels,
        }
    }

    /// Map [`Matched`] from one pattern to another.
    ///
    /// This is useful for normalizing patterns that have been matched against a different set of
    /// meta variables.
    #[instrument(level = "trace", ret)]
    fn normalize(matched_map: &MatchedMap, matched_from: &Matched<'tcx>, label_map_from: &pat::LabelMap) -> Self {
        let ty_vars = IndexVec::from_fn_n(
            |i| matched_from.ty_vars[matched_map.ty_vars[i]],
            matched_map.ty_vars.len(),
        );
        let const_vars = IndexVec::from_fn_n(
            |i| matched_from.const_vars[matched_map.const_vars[i]],
            matched_map.const_vars.len(),
        );
        let place_vars = IndexVec::from_fn_n(
            |i| matched_from.place_vars[matched_map.place_vars[i]],
            matched_map.place_vars.len(),
        );
        let labels: SortedMap<_, _> = label_map_from
            .iter()
            .map(|(label, spanned)| {
                let mapped_label = *matched_map.labels.get(label).unwrap_or(label);
                match spanned {
                    Spanned::Location(location) => (mapped_label, NormalizedSpanned::Location(matched_from[*location])),
                    Spanned::Local(local) => (mapped_label, NormalizedSpanned::Local(matched_from[*local])),
                    Spanned::Body => (mapped_label, NormalizedSpanned::Body),
                    Spanned::Output => (mapped_label, NormalizedSpanned::Output),
                }
            })
            .collect();

        NormalizedMatched {
            ty_vars,
            const_vars,
            place_vars,
            extra: labels,
        }
    }

    /// Map [`Matched`] from one pattern to another.
    ///
    /// This is useful for normalizing patterns that have been matched against a different set of
    /// meta variables.
    #[instrument(level = "trace", ret)]
    fn map(self, matched_map: &MatchedMap) -> Self {
        let ty_vars = IndexVec::from_fn_n(|i| self.ty_vars[matched_map.ty_vars[i]], matched_map.ty_vars.len());
        let const_vars = IndexVec::from_fn_n(
            |i| self.const_vars[matched_map.const_vars[i]],
            matched_map.const_vars.len(),
        );
        let place_vars = IndexVec::from_fn_n(
            |i| self.place_vars[matched_map.place_vars[i]],
            matched_map.place_vars.len(),
        );
        let labels: SortedMap<_, _> = self
            .extra
            .iter()
            .map(|(label, spanned)| (*matched_map.labels.get(label).unwrap_or(label), *spanned))
            .collect();

        NormalizedMatched {
            ty_vars,
            const_vars,
            place_vars,
            extra: labels,
        }
    }

    #[instrument(level = "trace", ret)]
    fn has_same_head(&self, other: &Self) -> bool {
        self.ty_vars.len() == other.ty_vars.len()
            && self.const_vars.len() == other.const_vars.len()
            && self.place_vars.len() == other.place_vars.len()
            && self.extra.len() == other.extra.len()
            && self
                .extra
                .iter()
                .zip(other.extra.iter())
                .all(|((label1, _), (label2, _))| label1 == label2)
    }
}

impl<'a, 'tcx> pat::Matched<'a, 'tcx, (&'a Body<'tcx>, &'a FnDecl<'tcx>, Option<Symbol>)> for NormalizedMatched<'tcx> {
    fn bottom_name(&self, (_, _, name): (&'a Body<'tcx>, &'a FnDecl<'tcx>, Option<Symbol>)) -> Option<Symbol> {
        name
    }
    fn bottom_span(&self, (body, _, _): (&'a Body<'tcx>, &'a FnDecl<'tcx>, Option<Symbol>)) -> Span {
        body.span
    }
    fn span(&self, (body, decl, _): (&'a Body<'tcx>, &'a FnDecl<'tcx>, Option<Symbol>), name: &str) -> Span {
        let labels = &self.extra;
        let symbol = Symbol::intern(name);
        debug_assert!(
            labels.contains_key(&symbol),
            "label `{name}` not found in pattern labels: {labels:?}",
        );
        labels[&symbol].span(body, decl)
    }
}
impl<'tcx> pat::MatchedMetaVars<'tcx> for NormalizedMatched<'tcx> {
    fn type_meta_var(&self, idx: pat::TyVarIdx) -> Ty<'tcx> {
        self.ty_vars[idx]
    }
    fn const_meta_var(&self, idx: pat::ConstVarIdx) -> Const<'tcx> {
        self.const_vars[idx]
    }
    fn place_meta_var(&self, idx: pat::PlaceVarIdx, bottom: LocalDefId) -> (LocalDefId, PlaceRef<'tcx>) {
        (bottom, self.place_vars[idx])
    }
}
