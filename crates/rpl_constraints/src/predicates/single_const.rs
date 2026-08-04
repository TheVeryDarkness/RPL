use rustc_middle::ty::{self, TyCtxt};

use crate::Const;

pub type SingleConstPredsFnPtr = for<'tcx> fn(TyCtxt<'tcx>, ty::TypingEnv<'tcx>, Const<'tcx>) -> bool;

/// A predicate that checks if a type is a null pointer.
#[instrument(level = "debug", skip(tcx, typing_env), ret)]
pub fn is_null_ptr<'tcx>(tcx: TyCtxt<'tcx>, typing_env: ty::TypingEnv<'tcx>, const_: Const<'tcx>) -> bool {
    const_.try_eval_scalar(tcx, typing_env).is_none_or(|value| {
        value
            .to_pointer(&tcx)
            .map(|ptr| {
                ptr.into_pointer_or_addr().map(|_| false).unwrap_or_else(|offset| {
                    trace!(?offset, "is_null_ptr: pointer has no provenance");
                    offset.bytes() == 0
                })
            })
            .unwrap_or_else(|_| false)
    })
}

/// Returns `false` when the const is proven zero (reject the match).
/// Returns `true` when non-zero or not evaluable (still lint variable lengths).
#[instrument(level = "debug", skip(tcx, typing_env), ret)]
pub fn is_nonzero<'tcx>(tcx: TyCtxt<'tcx>, typing_env: ty::TypingEnv<'tcx>, const_: Const<'tcx>) -> bool {
    match const_.try_eval_scalar_int(tcx, typing_env) {
        Some(v) => !v.is_null(),
        None => true,
    }
}
