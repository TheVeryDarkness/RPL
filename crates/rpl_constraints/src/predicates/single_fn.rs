use rustc_hir::def_id::LocalDefId;
use rustc_middle::ty::TyCtxt;

pub type SingleFnPredsFnPtr = for<'tcx> fn(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool;

/// Check if self_ty's trait bounds are all safe.
#[instrument(level = "debug", skip(tcx), ret)]
pub fn requires_monomorphization<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    tcx.generics_of(def_id).requires_monomorphization(tcx)
}
