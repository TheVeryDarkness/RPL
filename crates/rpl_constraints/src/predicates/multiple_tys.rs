use rustc_middle::ty::{self, Ty, TyCtxt};

pub type MultipleTysPredsFnPtr = for<'tcx> fn(TyCtxt<'tcx>, ty::TypingEnv<'tcx>, Vec<Ty<'tcx>>) -> bool;

/// Check if all tys' sizes are the same
pub fn same_size<'tcx>(tcx: TyCtxt<'tcx>, typing_env: ty::TypingEnv<'tcx>, tys: Vec<Ty<'tcx>>) -> bool {
    let mut layout_res = tys.iter().map(|ty| tcx.layout_of(typing_env.as_query_input(*ty)));
    if layout_res.any(|layout| layout.is_err()) {
        return false;
    }
    // if all layouts are ok, check if all sizes are the same
    let layouts = layout_res
        .map(|layout| layout.unwrap().layout.size())
        .collect::<Vec<_>>();
    layouts.windows(2).all(|w| w[0] == w[1])
}

/// Check if all tys' alignments are the same
pub fn same_abi_and_pref_align<'tcx>(tcx: TyCtxt<'tcx>, typing_env: ty::TypingEnv<'tcx>, tys: Vec<Ty<'tcx>>) -> bool {
    let mut layout_res = tys.iter().map(|ty| tcx.layout_of(typing_env.as_query_input(*ty)));
    if layout_res.any(|layout| layout.is_err()) {
        return false;
    }
    // if all layouts are ok, check if all alignments are the same
    let layouts = layout_res
        .map(|layout| layout.unwrap().layout.align())
        .collect::<Vec<_>>();
    layouts.windows(2).all(|w| w[0] == w[1])
}
