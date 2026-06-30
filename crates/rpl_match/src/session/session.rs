use rpl_context::pat::{self, PatternItem};
use rustc_hir::def_id::LocalDefId;

use crate::session::collect::MatchCollectCtxt;
use crate::session::config::SessionConfig;
use crate::session::csp::CspSolver;
use crate::session::slot::{
    AdtSlotDesc, CrateItemIndex, FnMatchContext, FnSlotCandidate, FnSlotDesc, MatchSlot, SessionResult, SlotAssignment,
    SlotCandidate, collect_slot_descs,
};

/// How slots inside one [`pat::RustItems`] block relate to each other.
///
/// This is distinct from OR across separate `patt { ... }` entries (e.g. the five
/// `uninit-vec` variants), which the driver matches independently and may all fire on
/// the same MIR site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustItemsMatchingMode {
    /// Single `fn _` slot: each function in the crate is tried independently.
    IndependentWildcardFn,
    /// One ADT slot plus one MIR-body fn: struct and fn bindings must agree, each fn tried
    /// independently (e.g. struct + `fn $pattern`).
    StructContextSingleFn,
    /// Multiple MIR-body fn slots with no ADT: OR alternatives that may overlap on the same site.
    AlternativeFns,
    /// Multiple ADT/fn slots or cross-slot metavar sharing: all slots via CSP
    /// (e.g. `multi_fn_shared_ty` `$Pair` + `$f1` + `$f2`).
    ConcurrentSlots,
}

fn mir_body_fn_slots<'a>(fn_slots: &'a [FnSlotDesc<'a>]) -> Vec<&'a FnSlotDesc<'a>> {
    fn_slots
        .iter()
        .filter(|desc| !desc.fn_pat.is_signature_only())
        .collect()
}

fn classify_rust_items_matching_mode(
    fn_slots: &[FnSlotDesc<'_>],
    adt_slots: &[AdtSlotDesc<'_>],
) -> RustItemsMatchingMode {
    let body_slots = mir_body_fn_slots(fn_slots);

    if adt_slots.is_empty() {
        if body_slots.len() == 1 && body_slots[0].optional {
            RustItemsMatchingMode::IndependentWildcardFn
        } else if body_slots.len() > 1 {
            RustItemsMatchingMode::AlternativeFns
        } else {
            RustItemsMatchingMode::ConcurrentSlots
        }
    } else if body_slots.len() == 1 && !body_slots[0].optional {
        RustItemsMatchingMode::StructContextSingleFn
    } else {
        RustItemsMatchingMode::ConcurrentSlots
    }
}

fn fn_slot_index(fn_slots: &[FnSlotDesc<'_>], slot: MatchSlot) -> usize {
    fn_slots
        .iter()
        .position(|desc| desc.slot == slot)
        .unwrap_or_else(|| panic!("unknown fn slot {slot:?}"))
}

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

        let adt_candidates: Vec<Vec<super::slot::AdtSlotCandidate<'tcx>>> = adt_slots
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

        let body_slots = mir_body_fn_slots(&fn_slots);

        match classify_rust_items_matching_mode(&fn_slots, &adt_slots) {
            RustItemsMatchingMode::IndependentWildcardFn => {
                let desc = body_slots[0];
                let slot_idx = fn_slot_index(&fn_slots, desc.slot);
                return self.enrich_and_postprocess(
                    index,
                    rust_items,
                    fn_candidates[slot_idx]
                        .iter()
                        .map(|c| self.session_result_from_fn(rust_items, desc.slot, c)),
                );
            },
            RustItemsMatchingMode::StructContextSingleFn => {
                let fn_desc = body_slots[0];
                let fn_slot_idx = fn_slot_index(&fn_slots, fn_desc.slot);
                let adt_desc = &adt_slots[0];
                return self.enrich_and_postprocess(
                    index,
                    rust_items,
                    adt_candidates[0].iter().flat_map(|adt| {
                        fn_candidates[fn_slot_idx].iter().filter_map(|c| {
                            let mut bindings = super::bindings::MetaBindings::new(rust_items.meta.as_ref());
                            if !bindings.merge_adt_ty_bindings(&adt.ty_bindings)
                                || !bindings.merge_snapshot(&c.snapshot)
                            {
                                return None;
                            }
                            Some(SessionResult {
                                assignments: vec![
                                    SlotAssignment {
                                        slot: adt_desc.slot,
                                        candidate: SlotCandidate::Adt(adt.clone()),
                                    },
                                    SlotAssignment {
                                        slot: fn_desc.slot,
                                        candidate: SlotCandidate::Fn(c.clone()),
                                    },
                                ],
                                bindings,
                                primary_fn: Some(FnMatchContext {
                                    def_id: c.def_id,
                                    fn_name: None,
                                    header: None,
                                    has_self: false,
                                    self_ty: None,
                                }),
                            })
                        })
                    }),
                );
            },
            RustItemsMatchingMode::AlternativeFns => {
                return self.enrich_and_postprocess(
                    index,
                    rust_items,
                    body_slots.iter().flat_map(|desc| {
                        let slot_idx = fn_slot_index(&fn_slots, desc.slot);
                        fn_candidates[slot_idx]
                            .iter()
                            .map(|c| self.session_result_from_fn(rust_items, desc.slot, c))
                    }),
                );
            },
            RustItemsMatchingMode::ConcurrentSlots => {},
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
        results = Self::deduplicate_fn_slot_permutations(results);
        self.enrich_results(index, &mut results);
        Self::deduplicate_results(rust_items.attr.should_deduplicate(), results)
    }

    fn session_result_from_fn(
        &self,
        rust_items: &'pcx pat::RustItems<'pcx>,
        slot: MatchSlot,
        candidate: &FnSlotCandidate<'tcx>,
    ) -> SessionResult<'tcx> {
        SessionResult {
            assignments: vec![SlotAssignment {
                slot,
                candidate: SlotCandidate::Fn(candidate.clone()),
            }],
            bindings: {
                let mut bindings = super::bindings::MetaBindings::new(rust_items.meta.as_ref());
                bindings.merge_snapshot(&candidate.snapshot);
                bindings
            },
            primary_fn: Some(FnMatchContext {
                def_id: candidate.def_id,
                fn_name: None,
                header: None,
                has_self: false,
                self_ty: None,
            }),
        }
    }

    fn deduplicate_fn_slot_permutations(results: Vec<SessionResult<'tcx>>) -> Vec<SessionResult<'tcx>> {
        let (single_fn, multi_fn): (Vec<_>, Vec<_>) = results
            .into_iter()
            .partition(|result| Self::fn_assignment_signature(result).len() <= 1);

        let mut kept_multi = Vec::new();
        for result in multi_fn {
            let signature = Self::fn_assignment_signature(&result);
            if kept_multi
                .iter()
                .all(|existing| Self::fn_assignment_signature(existing) != signature)
            {
                kept_multi.push(result);
            }
        }

        let mut kept = single_fn;
        kept.extend(kept_multi);
        kept
    }

    fn fn_assignment_signature(result: &SessionResult<'tcx>) -> Vec<LocalDefId> {
        let mut defs: Vec<_> = result
            .assignments
            .iter()
            .filter_map(|a| match &a.candidate {
                SlotCandidate::Fn(c) => Some(c.def_id),
                SlotCandidate::Adt(_) => None,
            })
            .collect();
        defs.sort_by_key(|def_id| def_id.local_def_index);
        defs
    }

    fn deduplicate_results(deduplicate: bool, results: Vec<SessionResult<'tcx>>) -> Vec<SessionResult<'tcx>> {
        if !deduplicate {
            return results;
        }
        let mut kept = Vec::new();
        for result in results {
            if kept
                .iter()
                .all(|existing: &SessionResult<'tcx>| !existing.equivalent(&result))
            {
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
                        .is_some_and(|(neg_def, neg_norm)| pos_def == neg_def && pos_norm == neg_norm)
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
            && self.primary_fn_slot() == other.primary_fn_slot()
            && match (self.primary_fn_candidate(), other.primary_fn_candidate()) {
                (Some(a), Some(b)) => a.def_id == b.def_id && a.normalized == b.normalized,
                (None, None) => true,
                _ => false,
            }
    }
}
