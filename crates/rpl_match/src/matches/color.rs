//! Check if the pattern statement matches MIR statement,
//! A.K.A. if we're using building blocks with the right color.

use rustc_abi::FieldIdx;
use rustc_middle::{mir, ty};

use crate::matches::MatchCtxt;
use crate::mir::pat;
use crate::statement::MatchStatement;
use crate::ty::MatchTy;

impl<'a, 'pcx, 'tcx> MatchStatement<'pcx, 'tcx> for MatchCtxt<'a, 'pcx, 'tcx> {
    fn body(&self) -> &mir::Body<'tcx> {
        self.cx.body
    }

    fn mir_pat(&self) -> &pat::FnPatternBody<'pcx> {
        self.cx.mir_pat
    }

    fn pat_cfg(&self) -> &crate::graph::PatControlFlowGraph {
        &self.cx.pat_cfg
    }
    fn pat_ddg(&self) -> &crate::graph::PatDataDepGraph {
        &self.cx.pat_ddg
    }
    fn mir_cfg(&self) -> &crate::graph::MirControlFlowGraph {
        self.cx.mir_cfg
    }
    fn mir_ddg(&self) -> &crate::graph::MirDataDepGraph {
        self.cx.mir_ddg
    }

    fn pat(&self) -> &'pcx pat::RustItems<'pcx> {
        self.cx.ty.pat
    }

    fn pcx(&self) -> rpl_context::PatCtxt<'pcx> {
        self.cx.ty.pcx
    }

    fn tcx(&self) -> rustc_middle::ty::TyCtxt<'tcx> {
        self.cx.ty.tcx
    }

    fn typing_env(&self) -> rustc_middle::ty::TypingEnv<'tcx> {
        self.cx.ty.typing_env
    }

    type MatchTy = Self;
    fn ty(&self) -> &Self::MatchTy {
        self
    }

    fn match_local(&self, pat: pat::Local, local: mir::Local) -> bool {
        self.matching.locals[pat].force_get_matched() == local
    }

    fn match_place_var(&self, pat: pat::PlaceVarIdx, place: mir::PlaceRef<'tcx>) -> bool {
        self.matching.place_vars[pat].force_get_matched() == place
    }

    fn get_place_ty_from_place_var(&self, var: pat::PlaceVarIdx) -> pat::PlaceTy<'pcx> {
        self.cx.get_place_ty_from_place_var(var)
        // pat::PlaceTy::from_ty(var.ty)
    }
}

impl<'a, 'pcx, 'tcx> MatchTy<'pcx, 'tcx> for MatchCtxt<'a, 'pcx, 'tcx> {
    fn pat(&self) -> &'pcx pat::RustItems<'pcx> {
        &self.cx.ty.pat
    }

    fn pcx(&self) -> rpl_context::PatCtxt<'pcx> {
        self.cx.ty.pcx
    }

    fn tcx(&self) -> rustc_middle::ty::TyCtxt<'tcx> {
        self.cx.ty.tcx
    }

    fn typing_env(&self) -> rustc_middle::ty::TypingEnv<'tcx> {
        self.cx.ty.typing_env
    }

    fn match_ty_var(&self, ty_var: pat::TyVar, ty: rustc_middle::ty::Ty<'tcx>) -> bool {
        self.matching.ty_vars[ty_var.idx].force_get_matched() == ty
    }

    fn match_ty_const_var(&self, const_var: pat::ConstVar<'pcx>, konst: rustc_middle::ty::Const<'tcx>) -> bool {
        let konst_matched = self.matching.const_vars[const_var.idx].force_get_matched();
        if let mir::Const::Ty(_, konst_matched) = konst_matched {
            konst_matched == konst
        } else {
            false
        }
    }

    fn match_const_var(&self, const_var: pat::ConstVar<'pcx>, konst: mir::Const<'tcx>) -> bool {
        self.matching.const_vars[const_var.idx].force_get_matched() == konst
    }

    #[expect(unused_variables)]
    fn match_adt_matches(&self, pat: rustc_span::Symbol, adt_match: crate::AdtMatch<'tcx>) -> bool {
        todo!()
    }

    #[expect(unused_variables)]
    fn for_variant_and_match(
        &self,
        adt_pat: rustc_span::Symbol,
        adt: rustc_middle::ty::AdtDef<'tcx>,
        // variant_idx_pat: Option<Symbol>,
        // variant_idx: Option<VariantIdx>,
        f: impl FnOnce(&pat::Variant<'pcx>, &crate::Candidates<FieldIdx>, &'tcx ty::VariantDef),
    ) {
        todo!()
    }
}
