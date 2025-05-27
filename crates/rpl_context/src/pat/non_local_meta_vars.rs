use either::Either;
use rpl_meta::collect_elems_separated_by_comma;
use rpl_meta::symbol_table::{GetType, WithPath};
use rpl_parser::generics::Choice3;
use rpl_parser::pairs;
use rustc_index::IndexVec;
use rustc_span::Symbol;
use std::ops::Deref;

use crate::PatCtxt;
use crate::pat::{ConstVar, ConstVarIdx, PlaceVar, PlaceVarIdx, Ty, TyPred, TyVar, TyVarIdx, with_path};

#[derive(Default, Debug)]
pub struct NonLocalMetaVars<'pcx> {
    pub ty_vars: IndexVec<TyVarIdx, TyVar<'pcx>>,
    pub const_vars: IndexVec<ConstVarIdx, ConstVar<'pcx>>,
    pub place_vars: IndexVec<PlaceVarIdx, PlaceVar<'pcx>>,
}

impl<'pcx> NonLocalMetaVars<'pcx> {
    pub fn add_ty_var(&mut self, name: Symbol, pred: &'pcx [&'pcx [Either<TyPred, TyPred>]]) {
        let idx = self.ty_vars.next_index();
        let ty_var = TyVar { idx, name, pred };
        self.ty_vars.push(ty_var);
    }
    pub fn add_const_var(&mut self, name: Symbol, ty: Ty<'pcx>) {
        let idx = self.const_vars.next_index();
        let const_var = ConstVar { idx, name, ty };
        self.const_vars.push(const_var);
    }
    pub fn add_place_var(&mut self, name: Symbol, ty: Ty<'pcx>) {
        let idx = self.place_vars.next_index();
        let place_var = PlaceVar { idx, name, ty };
        self.place_vars.push(place_var);
    }

    pub fn from_meta_decls(
        meta_decls: Option<WithPath<'pcx, &'pcx pairs::MetaVariableDeclList<'pcx>>>,
        pcx: PatCtxt<'pcx>,
        fn_sym_tab: &'pcx impl GetType<'pcx>,
    ) -> Self {
        let mut meta = Self::default();
        if let Some(decls) = meta_decls
            && let p = decls.path
            && let Some(decls) = decls.get_matched().1
        {
            let decls = collect_elems_separated_by_comma!(decls).collect::<Vec<_>>();
            // handle the type meta variable first
            let mut type_vars = Vec::new();
            let mut konst_vars = Vec::new();
            let mut place_vars = Vec::new();
            for decl in decls {
                let (ident, _, ty) = decl.get_matched();
                let ident = Symbol::intern(ident.span.as_str());
                match ty.deref() {
                    Choice3::_0(_ty) => type_vars.push(ident),
                    Choice3::_1(konst) => konst_vars.push((ident, konst)),
                    Choice3::_2(place) => place_vars.push((ident, place)),
                }
            }
            for ident in type_vars {
                meta.add_ty_var(ident, &[]);
            }
            for (ident, konst) in konst_vars {
                let ty = Ty::from(with_path(p, konst.get_matched().2), pcx, fn_sym_tab);
                meta.add_const_var(ident, ty);
            }
            for (ident, place) in place_vars {
                let ty = Ty::from(with_path(p, place.get_matched().2), pcx, fn_sym_tab);
                meta.add_place_var(ident, ty);
            }
        }
        meta
    }
}
