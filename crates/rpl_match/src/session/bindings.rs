use rpl_constraints::Const;
use rpl_context::pat::{ConstVarIdx, NonLocalMetaVars, PlaceVarIdx, TyVarIdx};
use rustc_index::IndexVec;
use rustc_middle::mir::PlaceRef;
use rustc_middle::ty::Ty;
use rustc_span::Symbol;

use crate::AdtFieldMap;
use crate::matches::artifact::NormalizedMatched;

/// Snapshot of metavar bindings projected onto a shared [`NonLocalMetaVars`] index space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingSnapshot<'tcx> {
    pub ty_vars: IndexVec<TyVarIdx, Ty<'tcx>>,
    pub const_vars: IndexVec<ConstVarIdx, Const<'tcx>>,
    pub place_vars: IndexVec<PlaceVarIdx, PlaceRef<'tcx>>,
    pub adt_fields: AdtFieldMap,
}

impl<'tcx> BindingSnapshot<'tcx> {
    pub fn from_normalized(matched: &NormalizedMatched<'tcx>) -> Self {
        Self {
            ty_vars: matched.ty_vars.clone(),
            const_vars: matched.const_vars.clone(),
            place_vars: matched.place_vars.clone(),
            adt_fields: matched.adt_fields.clone(),
        }
    }

    /// Build a partial snapshot containing only type metavar bindings (e.g. from ADT matching).
    pub fn from_ty_vars(meta: &NonLocalMetaVars<'_>, ty_vars: IndexVec<TyVarIdx, Ty<'tcx>>) -> Self {
        debug_assert_eq!(ty_vars.len(), meta.ty_vars.len());
        Self {
            ty_vars,
            const_vars: IndexVec::from_fn_n(
                |i| {
                    // Placeholder: ADT-only matching does not bind const vars yet.
                    let _ = i;
                    Const::Param(rustc_middle::ty::ParamConst {
                        index: 0,
                        name: Symbol::intern("_"),
                    })
                },
                meta.const_vars.len(),
            ),
            place_vars: IndexVec::from_fn_n(
                |i| {
                    let _ = i;
                    PlaceRef {
                        local: rustc_middle::mir::Local::from_u32(0),
                        projection: &[],
                    }
                },
                meta.place_vars.len(),
            ),
            adt_fields: AdtFieldMap::default(),
        }
    }
}

impl<'tcx> BindingSnapshot<'tcx> {
    /// Merge only type metavar rows into global bindings (ignores placeholder const/place rows).
    pub fn merge_ty_vars_into(&self, bindings: &mut MetaBindings<'tcx>) -> bool {
        bindings.merge_ty_vars(&self.ty_vars)
    }
}

/// Global metavar environment shared across all slots in a match session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaBindings<'tcx> {
    pub ty_vars: IndexVec<TyVarIdx, Option<Ty<'tcx>>>,
    pub const_vars: IndexVec<ConstVarIdx, Option<Const<'tcx>>>,
    pub place_vars: IndexVec<PlaceVarIdx, Option<PlaceRef<'tcx>>>,
    pub adt_fields: AdtFieldMap,
}

impl<'tcx> MetaBindings<'tcx> {
    pub fn new(meta: &NonLocalMetaVars<'_>) -> Self {
        Self {
            ty_vars: IndexVec::from_elem_n(None, meta.ty_vars.len()),
            const_vars: IndexVec::from_elem_n(None, meta.const_vars.len()),
            place_vars: IndexVec::from_elem_n(None, meta.place_vars.len()),
            adt_fields: AdtFieldMap::default(),
        }
    }

    pub fn merge_snapshot(&mut self, snapshot: &BindingSnapshot<'tcx>) -> bool {
        self.merge_ty_vars(&snapshot.ty_vars)
            && self.merge_const_vars(&snapshot.const_vars)
            && self.merge_place_vars(&snapshot.place_vars)
            && self.merge_adt_fields(&snapshot.adt_fields)
    }

    pub fn merge_normalized(&mut self, matched: &NormalizedMatched<'tcx>) -> bool {
        self.merge_snapshot(&BindingSnapshot::from_normalized(matched))
    }

    /// Merge type metavar bindings from ADT slot matching (shape/ty only).
    pub fn merge_adt_ty_bindings(&mut self, ty_bindings: &IndexVec<TyVarIdx, Ty<'tcx>>) -> bool {
        self.merge_ty_vars(ty_bindings)
    }

    pub fn merge_adt_fields(&mut self, fields: &AdtFieldMap) -> bool {
        for (key, idx) in fields {
            match self.adt_fields.get(key) {
                None => {
                    self.adt_fields.insert(*key, *idx);
                },
                Some(existing) if *existing == *idx => {},
                Some(_) => return false,
            }
        }
        true
    }

    pub(crate) fn merge_ty_vars(&mut self, vars: &IndexVec<TyVarIdx, Ty<'tcx>>) -> bool {
        for (idx, value) in vars.iter_enumerated() {
            if Self::should_skip_ty_binding(*value) {
                continue;
            }
            match &self.ty_vars[idx] {
                None => self.ty_vars[idx] = Some(value.clone()),
                Some(existing) if existing == value => {},
                Some(_) => return false,
            }
        }
        true
    }

    /// ADT definition matching may bind type metas to generic parameters; concrete
    /// bindings come from monomorphized function slots instead.
    fn should_skip_ty_binding(ty: Ty<'tcx>) -> bool {
        matches!(
            ty.kind(),
            rustc_middle::ty::TyKind::Param(_) | rustc_middle::ty::TyKind::Never
        )
    }

    fn merge_const_vars(&mut self, vars: &IndexVec<ConstVarIdx, Const<'tcx>>) -> bool {
        merge_index_vec(&mut self.const_vars, vars, |a, b| a == b)
    }

    fn merge_place_vars(&mut self, vars: &IndexVec<PlaceVarIdx, PlaceRef<'tcx>>) -> bool {
        merge_index_vec(&mut self.place_vars, vars, |a, b| a == b)
    }

    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.ty_vars == other.ty_vars
            && self.const_vars == other.const_vars
            && self.place_vars == other.place_vars
            && self.adt_fields == other.adt_fields
    }
}

pub(crate) fn merge_index_vec<I: rustc_index::Idx, T: Clone + PartialEq>(
    target: &mut IndexVec<I, Option<T>>,
    source: &IndexVec<I, T>,
    eq: impl Fn(&T, &T) -> bool,
) -> bool {
    for (idx, value) in source.iter_enumerated() {
        match &target[idx] {
            None => target[idx] = Some(value.clone()),
            Some(existing) if eq(existing, value) => {},
            Some(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use rustc_index::IndexVec;
    use rustc_span::Symbol;

    use super::{MetaBindings, merge_index_vec};
    use crate::AdtFieldMap;

    #[test]
    fn merge_index_vec_consistent() {
        let mut target: IndexVec<u32, Option<u32>> = IndexVec::from_elem_n(None, 2);
        let source: IndexVec<u32, u32> = IndexVec::from_raw(vec![1, 2]);
        assert!(merge_index_vec(&mut target, &source, |a, b| a == b));
        assert_eq!(target[0], Some(1));
        assert_eq!(target[1], Some(2));
        assert!(merge_index_vec(&mut target, &source, |a, b| a == b));
    }

    #[test]
    fn merge_index_vec_conflict() {
        let mut target: IndexVec<u32, Option<u32>> = IndexVec::from_elem_n(None, 1);
        let a: IndexVec<u32, u32> = IndexVec::from_raw(vec![1]);
        let b: IndexVec<u32, u32> = IndexVec::from_raw(vec![2]);
        assert!(merge_index_vec(&mut target, &a, |x, y| x == y));
        assert!(!merge_index_vec(&mut target, &b, |x, y| x == y));
    }

    #[test]
    fn merge_adt_fields_consistent_and_conflict() {
        rustc_span::create_session_if_not_set_then(rustc_span::edition::LATEST_STABLE_EDITION, |_| {
            use rustc_abi::FieldIdx;

            let mut bindings = MetaBindings {
                ty_vars: IndexVec::new(),
                const_vars: IndexVec::new(),
                place_vars: IndexVec::new(),
                adt_fields: AdtFieldMap::default(),
            };
            let adt = Symbol::intern("$SlabT");
            let len = Symbol::intern("$len");
            let mem = Symbol::intern("$mem");
            let mut fn_fields = AdtFieldMap::default();
            fn_fields.insert((adt, len), FieldIdx::from_u32(1));
            fn_fields.insert((adt, mem), FieldIdx::from_u32(2));
            assert!(bindings.merge_adt_fields(&fn_fields));

            let mut conflicting = AdtFieldMap::default();
            conflicting.insert((adt, len), FieldIdx::from_u32(0));
            assert!(!bindings.merge_adt_fields(&conflicting));
        });
    }
}
