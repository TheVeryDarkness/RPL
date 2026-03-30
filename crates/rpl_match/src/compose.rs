use std::fmt;
use std::ops::DerefMut;

use either::Either;
use rpl_constraints::predicates::BodyInfoCache;
use rpl_context::PatCtxt;
use rpl_context::pat::{MatchedLocalVars, MatchedMetaVars};
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_hir::FnHeader;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::mir;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::Symbol;

use crate::graph::{MirControlFlowGraph, MirDataDepGraph};
use crate::mir::pat;
use crate::mir::pat::PatternItem;
use crate::normalized::NormalizedMatched;
use crate::predicate_evaluator::PredicateEvaluator;

// body: &'a mir::Body<'tcx>,
// has_self: bool,
// self_ty: Option<ty::Ty<'tcx>>,
// mir_cfg: &'a MirControlFlowGraph,
// mir_ddg: &'a MirDataDepGraph,
pub trait MatchComposedPattern<'a, 'pcx, 'tcx, Cx: Copy> {
    type Matched: fmt::Debug + MatchedMetaVars<'tcx> + MatchedLocalVars<'tcx>;
    type NormalizedMatched: fmt::Debug + NormalizedMatched<'tcx, Matched = Self::Matched>;
    fn pcx(&self) -> PatCtxt<'pcx>;
    fn tcx(&self) -> TyCtxt<'tcx>;
    fn body_caches(&self) -> impl DerefMut<Target = FxHashMap<DefId, BodyInfoCache>>;

    fn check_mir(
        tcx: TyCtxt<'tcx>,
        pcx: PatCtxt<'pcx>,
        pat: &'pcx pat::RustItems<'pcx>,
        pat_name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        cx: Cx,
    ) -> Vec<Self::Matched>;

    #[expect(clippy::too_many_arguments)]
    #[instrument(level = "trace", skip(self, rpl_rust_items, header, cx), fields(pat_name = ?name))]
    fn impl_matched(
        &self,
        name: Symbol,
        rpl_rust_items: &'pcx pat::RustItems<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        let iter = rpl_rust_items.impls.values().flat_map(move |impl_pat| {
            // FIXME: check impl_pat.ty and impl_pat.trait_id
            impl_pat
                .fns
                .values()
                .filter(move |fn_pat| self.filter(name, fn_pat, def_id, header, cx))
                .filter_map(move |fn_pat| Some((fn_pat, fn_pat.extra_span(self.tcx(), def_id)?)))
                .flat_map(move |(fn_pat, attr_map)| {
                    // FIXME: sometimes we need to check function name
                    // if *fn_name != impl_item.ident.name {
                    //     continue;
                    // }

                    Self::check_mir(self.tcx(), self.pcx(), rpl_rust_items, name, fn_pat, cx)
                        .into_iter()
                        .filter(move |matched| self.check_constraints(name, fn_pat, def_id, matched, cx))
                        .map(move |matched| {
                            let labels = &fn_pat.expect_body().labels;
                            (matched, labels, attr_map.clone())
                        })
                })
                .map(move |(matched, label_map, attr_map)| {
                    Self::NormalizedMatched::new(def_id, &matched, label_map, &attr_map)
                })
        });
        rpl_rust_items.post_process(iter)
    }

    #[instrument(level = "trace", skip(self, pat_op, header, cx), fields(pat_name = ?name))]
    #[expect(clippy::too_many_arguments)]
    fn impl_matched_pat_op(
        &self,
        name: Symbol,
        pat_op: &pat::PatternOperation<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        let positive: Vec<_> = pat_op
            .positive
            .iter()
            .flat_map(|positive| {
                self.impl_matched_pat_item(positive.0, positive.1, def_id, header, cx)
                    .map(|matched| matched.map(&positive.2))
            })
            .collect();
        let negative: FxHashSet<_> = pat_op
            .negative
            .iter()
            .flat_map(|negative| {
                self.impl_matched_pat_item(negative.0, negative.1, def_id, header, cx)
                    .map(|matched| matched.map(&negative.2))
            })
            .collect();
        debug!(?positive, ?negative, "impl_matched_pat_op");

        let iter = positive
            .into_iter()
            .filter(move |matched| {
                debug_assert!(negative.iter().all(|neg| neg.has_same_head(matched)));
                !negative.contains(matched)
            })
            .collect::<Vec<_>>()
            .into_iter();

        pat_op.post_process(iter)
    }

    #[expect(clippy::too_many_arguments)]
    fn impl_matched_pat_item(
        &self,
        name: Symbol,
        pat_item: &'pcx PatternItem<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        match pat_item {
            PatternItem::RustItems(rust_items) => Either::Left(self.impl_matched(name, rust_items, def_id, header, cx)),
            PatternItem::RPLPatternOperation(pat_op) => {
                Either::Right(self.impl_matched_pat_op(name, pat_op, def_id, header, cx))
            },
        }
    }

    #[expect(clippy::too_many_arguments)]
    #[instrument(level = "trace", skip(self, rpl_rust_items, header, cx), fields(pat_name = ?name))]
    fn fn_matched(
        &self,
        name: Symbol,
        rpl_rust_items: &'pcx pat::RustItems<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        let iter = rpl_rust_items
            .fns
            .iter()
            .filter(move |fn_pat| self.filter(name, fn_pat, def_id, header, cx))
            .filter_map(move |fn_pat| Some((fn_pat, fn_pat.extra_span(self.tcx(), def_id)?)))
            .flat_map(move |(fn_pat, attr_map)| {
                Self::check_mir(self.tcx(), self.pcx(), rpl_rust_items, name, fn_pat, cx)
                    .into_iter()
                    .filter(move |matched| self.check_constraints(name, fn_pat, def_id, matched, cx))
                    .map(move |matched| {
                        let labels = &fn_pat.expect_body().labels;
                        (matched, labels, attr_map.clone())
                    })
            })
            .map(move |(matched, label_map, attr_map)| NormalizedMatched::new(def_id, &matched, label_map, &attr_map));

        rpl_rust_items.post_process(iter)
    }

    #[instrument(level = "trace", skip(self, pat_op, header, cx), fields(pat_name = ?name))]
    #[expect(clippy::too_many_arguments)]
    fn fn_matched_pat_op(
        &self,
        name: Symbol,
        pat_op: &pat::PatternOperation<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        let positive: Vec<_> = pat_op
            .positive
            .iter()
            .flat_map(|positive| {
                self.fn_matched_pat_item(positive.0, positive.1, def_id, header, cx)
                    .map(|matched| matched.map(&positive.2))
            })
            .collect();
        let negative: FxHashSet<_> = pat_op
            .negative
            .iter()
            .flat_map(|negative| {
                self.fn_matched_pat_item(negative.0, negative.1, def_id, header, cx)
                    .map(|matched| matched.map(&negative.2))
            })
            .collect();
        debug!(?positive, ?negative, "impl_matched_pat_op");

        let iter = positive
            .into_iter()
            .filter(move |matched| {
                debug_assert!(negative.iter().all(|neg| neg.has_same_head(matched)));
                !negative.contains(matched)
            })
            .collect::<Vec<_>>()
            .into_iter();

        pat_op.post_process(iter)
    }

    #[expect(clippy::too_many_arguments)]
    fn fn_matched_pat_item(
        &self,
        name: Symbol,
        pat_item: &'pcx PatternItem<'pcx>,
        def_id: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> impl Iterator<Item = Self::NormalizedMatched> {
        match pat_item {
            PatternItem::RustItems(rust_items) => Either::Left(self.fn_matched(name, rust_items, def_id, header, cx)),
            PatternItem::RPLPatternOperation(pat_op) => {
                Either::Right(self.fn_matched_pat_op(name, pat_op, def_id, header, cx))
            },
        }
    }

    fn check_constraints(
        &self,
        name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        bottom: LocalDefId,
        matched: &Self::Matched,
        cx: Cx,
    ) -> bool;

    fn filter(
        &self,
        name: Symbol,
        fn_pat: &pat::FnPattern<'pcx>,
        bottom: LocalDefId,
        header: Option<FnHeader>,
        cx: Cx,
    ) -> bool;
}
