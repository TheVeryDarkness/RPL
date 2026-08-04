use rpl_constraints::Const;
use rpl_context::pat::{self, Matched};
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_middle::mir::{Body, PlaceRef};
use rustc_middle::ty::Ty;
use rustc_span::Span;
use rustc_span::source_map::SourceMap;

use crate::matches::artifact::NormalizedMatched;
use crate::session::bindings::MetaBindings;
use crate::session::slot::{MatchSlot, SessionResult, SlotCandidate};

/// Adapter for diagnostics spanning multiple session assignments.
pub struct MultiMatched<'a, 'tcx> {
    pub bindings: &'a MetaBindings<'tcx>,
    pub normalized: &'a NormalizedMatched<'tcx>,
}

impl std::fmt::Debug for MultiMatched<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiMatched").finish_non_exhaustive()
    }
}

impl<'a, 'tcx> MultiMatched<'a, 'tcx> {
    pub fn new(bindings: &'a MetaBindings<'tcx>, normalized: &'a NormalizedMatched<'tcx>) -> Self {
        Self { bindings, normalized }
    }

    pub fn from_session(result: &'a SessionResult<'tcx>, normalized: &'a NormalizedMatched<'tcx>) -> Self {
        Self::new(&result.bindings, normalized)
    }
}

impl<'tcx> Matched<'tcx> for MultiMatched<'_, 'tcx> {
    fn span(&self, body: &Body<'tcx>, decl: &FnDecl<'tcx>, name: &str, source_map: &SourceMap) -> Span {
        self.normalized.span(body, decl, name, source_map)
    }

    fn try_span(&self, body: &Body<'tcx>, decl: &FnDecl<'tcx>, name: &str, source_map: &SourceMap) -> Option<Span> {
        self.normalized.try_span(body, decl, name, source_map)
    }

    fn type_meta_var(&self, idx: pat::TyVarIdx) -> Ty<'tcx> {
        if let Some(ty) = self.bindings.ty_vars.get(idx).and_then(|t| *t) {
            return ty;
        }
        self.normalized.type_meta_var(idx)
    }

    fn const_meta_var(&self, idx: pat::ConstVarIdx) -> Const<'tcx> {
        if let Some(c) = self.bindings.const_vars.get(idx).and_then(|c| *c) {
            return c;
        }
        self.normalized.const_meta_var(idx)
    }

    fn place_meta_var(&self, idx: pat::PlaceVarIdx) -> PlaceRef<'tcx> {
        if let Some(p) = self.bindings.place_vars.get(idx).and_then(|p| *p) {
            return p;
        }
        self.normalized.place_meta_var(idx)
    }
}

/// Owned lint payload that can be sorted before emission.
pub struct OwnedLintMatch<'tcx> {
    pub def_id: LocalDefId,
    pub primary_slot: MatchSlot,
    pub normalized: NormalizedMatched<'tcx>,
    pub bindings: MetaBindings<'tcx>,
}

impl<'tcx> OwnedLintMatch<'tcx> {
    pub fn as_matched(&self) -> MultiMatched<'_, 'tcx> {
        MultiMatched::new(&self.bindings, &self.normalized)
    }
}

/// Emit lint target for a session result (def_id + matched view).
pub struct SessionLintTarget<'a, 'tcx> {
    pub def_id: LocalDefId,
    pub owned: OwnedLintMatch<'tcx>,
    _result: std::marker::PhantomData<&'a SessionResult<'tcx>>,
}

impl<'tcx> SessionResult<'tcx> {
    pub fn lint_targets<'a>(&'a self) -> Vec<SessionLintTarget<'a, 'tcx>> {
        let has_mir_body_fn = self.assignments.iter().any(|a| match &a.candidate {
            SlotCandidate::Fn(c) => !c.matched.basic_blocks.is_empty(),
            SlotCandidate::Adt(_) => false,
        });

        self.assignments
            .iter()
            .filter_map(|a| match &a.candidate {
                SlotCandidate::Fn(c) => {
                    // Skip signature-only companions when another slot carried a MIR body match.
                    if has_mir_body_fn && c.matched.basic_blocks.is_empty() {
                        return None;
                    }
                    Some(SessionLintTarget {
                        def_id: c.def_id,
                        owned: OwnedLintMatch {
                            def_id: c.def_id,
                            primary_slot: a.slot,
                            normalized: c.normalized.clone(),
                            bindings: self.bindings.clone(),
                        },
                        _result: std::marker::PhantomData,
                    })
                },
                SlotCandidate::Adt(_) => None,
            })
            .collect()
    }
}
