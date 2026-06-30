use rpl_context::pat;

use crate::session::bindings::MetaBindings;
use crate::session::config::SessionConfig;
use crate::session::slot::{
    AdtSlotDesc, FnMatchContext, FnSlotDesc, MatchSlot, SessionResult, SlotAssignment, SlotCandidate,
};

/// Backtracking CSP solver for full M:N consistent matching across slots.
pub struct CspSolver<'a, 'tcx> {
    config: SessionConfig,
    meta: &'a pat::NonLocalMetaVars<'a>,
    fn_slots: &'a [FnSlotDesc<'a>],
    adt_slots: &'a [AdtSlotDesc<'a>],
    fn_candidates: &'a [Vec<super::slot::FnSlotCandidate<'tcx>>],
    adt_candidates: &'a [Vec<super::slot::AdtSlotCandidate<'tcx>>],
}

impl<'a, 'pcx, 'tcx> CspSolver<'a, 'tcx> {
    pub fn new(
        config: SessionConfig,
        meta: &'a pat::NonLocalMetaVars<'pcx>,
        fn_slots: &'a [FnSlotDesc<'pcx>],
        adt_slots: &'a [AdtSlotDesc<'pcx>],
        fn_candidates: &'a [Vec<super::slot::FnSlotCandidate<'tcx>>],
        adt_candidates: &'a [Vec<super::slot::AdtSlotCandidate<'tcx>>],
    ) -> Self {
        Self {
            config,
            meta,
            fn_slots,
            adt_slots,
            fn_candidates,
            adt_candidates,
        }
    }

    pub fn solve(&self) -> Vec<SessionResult<'tcx>> {
        let mut results = Vec::new();
        let mut bindings = MetaBindings::new(self.meta);
        let mut assignments = Vec::new();
        let mut used_defs = Vec::new();

        self.solve_adt_slots(0, &mut bindings, &mut assignments, &mut used_defs, &mut results);
        results
    }

    fn solve_adt_slots(
        &self,
        idx: usize,
        bindings: &mut MetaBindings<'tcx>,
        assignments: &mut Vec<SlotAssignment<'tcx>>,
        used_defs: &mut Vec<LocalDefId>,
        results: &mut Vec<SessionResult<'tcx>>,
    ) {
        if idx == self.adt_slots.len() {
            self.solve_fn_slots(0, bindings, assignments, used_defs, results);
            return;
        }

        let desc = &self.adt_slots[idx];
        let slot_idx = self.adt_slot_index(desc.slot);
        if self.adt_candidates[slot_idx].is_empty() {
            self.solve_adt_slots(idx + 1, bindings, assignments, used_defs, results);
            return;
        }
        for candidate in &self.adt_candidates[slot_idx] {
            let mut trial = bindings.clone();
            if !trial.merge_adt_ty_bindings(&candidate.ty_bindings) {
                continue;
            }
            assignments.push(SlotAssignment {
                slot: desc.slot,
                candidate: SlotCandidate::Adt(candidate.clone()),
            });
            self.solve_adt_slots(idx + 1, &mut trial, assignments, used_defs, results);
            assignments.pop();
        }
    }

    fn solve_fn_slots(
        &self,
        idx: usize,
        bindings: &mut MetaBindings<'tcx>,
        assignments: &mut Vec<SlotAssignment<'tcx>>,
        used_defs: &mut Vec<LocalDefId>,
        results: &mut Vec<SessionResult<'tcx>>,
    ) {
        if idx == self.fn_slots.len() {
            self.push_result(bindings, assignments, used_defs, results);
            return;
        }

        if self.config.max_results > 0 && results.len() >= self.config.max_results {
            return;
        }

        let desc = &self.fn_slots[idx];
        let slot_idx = desc.slot.fn_index().unwrap();

        if desc.optional {
            // `fn _`: skip this slot (zero matches) and continue.
            self.solve_fn_slots(idx + 1, bindings, assignments, used_defs, results);

            for candidate in &self.fn_candidates[slot_idx] {
                if used_defs.contains(&candidate.def_id) {
                    continue;
                }
                let mut trial = bindings.clone();
                if !trial.merge_snapshot(&candidate.snapshot) {
                    continue;
                }
                used_defs.push(candidate.def_id);
                assignments.push(SlotAssignment {
                    slot: desc.slot,
                    candidate: SlotCandidate::Fn(candidate.clone()),
                });
                self.solve_fn_slots(idx + 1, &mut trial, assignments, used_defs, results);
                assignments.pop();
                used_defs.pop();
            }
        } else {
            for candidate in &self.fn_candidates[slot_idx] {
                if used_defs.contains(&candidate.def_id) {
                    continue;
                }
                let mut trial = bindings.clone();
                if !trial.merge_snapshot(&candidate.snapshot) {
                    continue;
                }
                used_defs.push(candidate.def_id);
                assignments.push(SlotAssignment {
                    slot: desc.slot,
                    candidate: SlotCandidate::Fn(candidate.clone()),
                });
                self.solve_fn_slots(idx + 1, &mut trial, assignments, used_defs, results);
                assignments.pop();
                used_defs.pop();
            }
        }
    }

    fn push_result(
        &self,
        bindings: &MetaBindings<'tcx>,
        assignments: &Vec<SlotAssignment<'tcx>>,
        _used_defs: &Vec<LocalDefId>,
        results: &mut Vec<SessionResult<'tcx>>,
    ) {
        let has_fn = assignments.iter().any(|a| matches!(a.candidate, SlotCandidate::Fn(_)));
        let has_adt = assignments.iter().any(|a| matches!(a.candidate, SlotCandidate::Adt(_)));
        if !has_fn && !has_adt {
            return;
        }
        let requires_fn = self.fn_slots.iter().any(|s| !s.optional);
        if requires_fn && !has_fn {
            return;
        }

        let primary_fn = assignments
            .iter()
            .find_map(|a| match &a.candidate {
                SlotCandidate::Fn(c) if !c.matched.basic_blocks.is_empty() => Some(FnMatchContext {
                    def_id: c.def_id,
                    fn_name: None,
                    header: None,
                    has_self: false,
                    self_ty: None,
                }),
                SlotCandidate::Fn(_) | SlotCandidate::Adt(_) => None,
            })
            .or_else(|| {
                assignments.iter().find_map(|a| match &a.candidate {
                    SlotCandidate::Fn(c) => Some(FnMatchContext {
                        def_id: c.def_id,
                        fn_name: None,
                        header: None,
                        has_self: false,
                        self_ty: None,
                    }),
                    SlotCandidate::Adt(_) => None,
                })
            });

        results.push(SessionResult {
            assignments: assignments.clone(),
            bindings: bindings.clone(),
            primary_fn,
        });
    }

    fn adt_slot_index(&self, slot: MatchSlot) -> usize {
        match slot {
            MatchSlot::Adt(name) => self.adt_slots.iter().position(|s| s.adt_pat_name == name).unwrap(),
            _ => panic!("expected ADT slot"),
        }
    }
}

impl MatchSlot {
    pub fn fn_index(self) -> Option<usize> {
        match self {
            MatchSlot::Fn(idx) => Some(idx),
            _ => None,
        }
    }
}

use rustc_hir::def_id::LocalDefId;
