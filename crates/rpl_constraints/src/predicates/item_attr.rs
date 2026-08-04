use rustc_hir::def_id::DefId;
use rustc_middle::ty::TyCtxt;
use rustc_span::Symbol;

pub type ItemAttrPredsFnPtr = for<'tcx> fn(TyCtxt<'tcx>, DefId, attrs: &[Symbol]) -> bool;

#[instrument(level = "debug", skip(tcx, def_id, attrs), ret)]
pub fn has_attr<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId, attrs: &[Symbol]) -> bool {
    tcx.has_attrs_with_path(def_id, attrs)
}
