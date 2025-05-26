use rustc_middle::mir::{Const, PlaceRef};
use rustc_middle::ty::Ty;
use rustc_span::Span;

use super::{ConstVarIdx, PlaceVarIdx, TyVarIdx};

pub trait Matched<'tcx> {
    fn named_span(&self, name: &str) -> Span;
    fn type_meta_var(&self, idx: TyVarIdx) -> Ty<'tcx>;
    fn const_meta_var(&self, idx: ConstVarIdx) -> Const<'tcx>;
    fn place_meta_var(&self, idx: PlaceVarIdx) -> PlaceRef<'tcx>;
}
