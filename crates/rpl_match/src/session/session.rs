use rpl_context::pat::{self, PatternItem};

use crate::session::collect::MatchCollectCtxt;
use crate::session::config::SessionConfig;
use crate::session::csp::CspSolver;
use crate::session::slot::{
    collect_slot_descs, CrateItemIndex, FnSlotCandidate, SessionResult, SlotCandidate,
};

/// Orchestrates candidate collection and CSP solving for one pattern item.
pub struct MatchSession<'a, 'pcx, 'tcx> {
    collect: MatchCollectCtxt<'a, 'pcx, 'tcx>,
    config: SessionConfig,
}

impl<'a, 'pcx, 'tcx> MatchSession<'a, 'pcx, 'tcx> {
    pub fn new(collect: MatchCollectCtxt<'a, 'pcx, 'tcx>, config: SessionConfig) -> Self {
        Self { collect, config }
    }

    pub fn with_defaults(collect: MatchCollectCtxt<'a, 'pcx, 'tcx>) -> Self {
        Self::new(collect, SessionConfig::default())
    }

    pub fn match_rust_items(
        &self,
        index: &CrateItemIndex,
        rust_items: &'pcx pat::RustItems<'pcx>,
    ) -> Vec<SessionResult<'tcx>> {
        let (fn_slots, adt_slots) = collect_slot_descs(rust_items);

        if fn_slots.is_empty() && adt_slots.is_empty() {
            return Vec::new();
        }

        let fn_candidates: Vec<Vec<FnSlotCandidate<'tcx>>> = fn_slots
            .iter()
            .map(|desc| {
                index
                    .fns
                    .iter()
                    .copied()
                    .flat_map(|item| self.collect.collect_fn_candidates(rust_items, desc.fn_pat, item))
                    .collect()
            })
            .collect();

        let adt_candidates: Vec<_> = adt_slots
            .iter()
            .map(|desc| {
                index
                    .adts
                    .iter()
                    .copied()
                    .flat_map(|item| self.collect.collect_adt_candidates(rust_items, *desc, item))
                    .collect()
            })
            .collect();

        if adt_slots.is_empty()
            && fn_slots.len() == 1
            && fn_slots[0].optional
            && fn_candidates.len() == 1
        {
            return self.enrich_and_postprocess(
                index,
                rust_items,
                fn_candidates[0].iter().map(|c| SessionResult {
                    assignments: vec![super::slot::SlotAssignment {
                        slot: fn_slots[0].slot,
                        candidate: SlotCandidate::Fn(c.clone()),
                    }],
                    bindings: {
                        let mut b = super::bindings::MetaBindings::new(rust_items.meta.as_ref());
                        b.merge_snapshot(&c.snapshot);
                        b
                    },
                    primary_fn: Some(super::slot::FnMatchContext {
                        def_id: c.def_id,
                        fn_name: None,
                        header: None,
                        has_self: false,
                        self_ty: None,
                    }),
                }),
            );
        }

        let solver = CspSolver::new(
            self.config,
            rust_items.meta.as_ref(),
            &fn_slots,
            &adt_slots,
            &fn_candidates,
            &adt_candidates,
        );
        let mut results = solver.solve();
        self.enrich_results(index, &mut results);
        Self::deduplicate_results(rust_items.attr.should_deduplicate(), results)
    }

    fn deduplicate_results(deduplicate: bool, results: Vec<SessionResult<'tcx>>) -> Vec<SessionResult<'tcx>> {
        if !deduplicate {
            return results;
        }
        let mut kept = Vec::new();
        for result in results {
            if kept.iter().all(|existing: &SessionResult<'tcx>| !existing.equivalent(&result)) {
                kept.push(result);
            }
        }
        kept
    }

    fn enrich_results(&self, index: &CrateItemIndex, results: &mut [SessionResult<'tcx>]) {
        for result in results.iter_mut() {
            if let Some(ctx) = &mut result.primary_fn {
                if let Some(item) = index.fns.iter().find(|i| i.def_id == ctx.def_id) {
                    ctx.fn_name = item.fn_name;
                    ctx.header = item.header;
                    ctx.has_self = item.has_self;
                }
                ctx.self_ty = index.self_ty(self.collect.tcx, ctx.def_id);
            }
        }
    }

    fn enrich_and_postprocess(
        &self,
        index: &CrateItemIndex,
        rust_items: &'pcx pat::RustItems<'pcx>,
        results: impl Iterator<Item = SessionResult<'tcx>>,
    ) -> Vec<SessionResult<'tcx>> {
        let mut results: Vec<_> = results.collect();
        self.enrich_results(index, &mut results);
        Self::deduplicate_results(rust_items.attr.should_deduplicate(), results)
    }

    pub fn match_pattern_item(
        &self,
        index: &CrateItemIndex,
        pat_item: &'pcx PatternItem<'pcx>,
    ) -> Vec<SessionResult<'tcx>> {
        match pat_item {
            PatternItem::RustItems(items) => self.match_rust_items(index, items),
            PatternItem::RPLPatternOperation(op) => {
                let results = self.match_pattern_operation(index, op);
                Self::deduplicate_results(op.attr.should_deduplicate(), results)
            },
        }
    }

    fn match_pattern_operation(
        &self,
        index: &CrateItemIndex,
        op: &pat::PatternOperation<'pcx>,
    ) -> Vec<SessionResult<'tcx>> {
        let positive: Vec<_> = op
            .positive
            .iter()
            .flat_map(|(_, item, map)| {
                self.match_pattern_item(index, item)
                    .into_iter()
                    .map(|result| result.map_bindings(map))
            })
            .collect();

        let negative: Vec<_> = op
            .negative
            .iter()
            .flat_map(|(_, item, map)| {
                self.match_pattern_item(index, item)
                    .into_iter()
                    .map(|result| result.map_bindings(map))
            })
            .collect();

        positive
            .into_iter()
            .filter(|pos| {
                let Some((pos_def, pos_norm)) = pos.operation_match_key() else {
                    return true;
                };
                !negative.iter().any(|neg| {
                    neg.operation_match_key()
                        .is_some_and(|(neg_def, neg_norm)| {
                            pos_def == neg_def && pos_norm == neg_norm
                        })
                })
            })
            .collect()
    }
}

impl<'tcx> SessionResult<'tcx> {
    fn map_bindings(self, map: &pat::MatchedMap) -> Self {
        let assignments = self
            .assignments
            .into_iter()
            .map(|mut a| {
                if let SlotCandidate::Fn(ref mut c) = a.candidate {
                    c.normalized = c.normalized.clone().map(map);
                    c.snapshot = super::bindings::BindingSnapshot::from_normalized(&c.normalized);
                }
                a
            })
            .collect();
        Self {
            assignments,
            bindings: self.bindings,
            primary_fn: self.primary_fn,
        }
    }

    pub fn equivalent(&self, other: &Self) -> bool {
        self.bindings.equivalent_to(&other.bindings)
            && self.assignments.len() == other.assignments.len()
            && match (self.primary_fn_candidate(), other.primary_fn_candidate()) {
                (Some(a), Some(b)) => a.def_id == b.def_id && a.normalized == b.normalized,
                (None, None) => true,
                _ => false,
            }
    }
}
