use either::Either;
use error::{DynamicError, DynamicErrorBuilder};
use rpl_meta::collect_elems_separated_by_comma;
use rpl_meta::symbol_table::{GetType, WithPath};
use rpl_meta::utils::Ident;
use rpl_parser::generics::{Choice2, Choice3, Choice4};
use rpl_parser::pairs;
use rustc_data_structures::fx::FxHashMap;
use rustc_index::IndexVec;
use rustc_middle::mir::Body;
use rustc_span::Symbol;
use std::ops::Deref;
use std::sync::Arc;

use crate::PatCtxt;

mod error;
mod item;
mod matched;
mod mir;
mod pretty;
mod ty;
mod utils;

pub use item::*;
pub use matched::Matched;
pub use mir::*;
pub use ty::*;

pub type Label = Symbol;
pub type LabelMap = FxHashMap<Label, mir::Location>;

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

/// Corresponds to a pattern file in RPL, not a pattern item.
pub struct Pattern<'pcx> {
    // FIXME: remove it
    pub pcx: PatCtxt<'pcx>,
    pub adts: FxHashMap<Symbol, Adt<'pcx>>,
    pub fns: Fns<'pcx>,
    #[expect(dead_code)]
    impls: Vec<Impl<'pcx>>,
    diag: FxHashMap<Symbol, DynamicErrorBuilder<'pcx>>,
}

impl<'pcx> Pattern<'pcx> {
    pub(crate) fn new(pcx: PatCtxt<'pcx>) -> Self {
        Self {
            pcx,
            adts: Default::default(),
            fns: Default::default(),
            impls: Default::default(),
            diag: Default::default(),
        }
    }

    // FIXME: remove it when pest parser is ready
    pub fn new_struct(&mut self, name: Symbol) -> &mut Adt<'pcx> {
        self.adts.entry(name).or_insert_with(Adt::new_struct)
        // .non_enum_variant_mut()
    }
    // FIXME: remove it when pest parser is ready
    pub fn new_enum(&mut self, name: Symbol) -> &mut Adt<'pcx> {
        self.adts.entry(name).or_insert_with(Adt::new_enum)
    }

    pub fn get_adt(&self, name: Symbol) -> Option<&Adt<'pcx>> {
        self.adts.get(&name)
    }

    pub fn get_diag<'tcx>(
        &self,
        pat_name: Symbol,
        label_map: &LabelMap,
        body: &Body<'tcx>,
        matched: &impl Matched<'tcx>,
    ) -> DynamicError {
        self.diag.get(&pat_name).unwrap().build(label_map, body, matched)
    }
}

impl<'pcx> Pattern<'pcx> {
    // pub fn from_parsed(
    //     pcx: PatCtxt<'pcx>,
    //     pat_item: WithPath<'pcx, &'pcx pairs::pattBlockItem<'pcx>>,
    //     symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
    // ) -> Self {
    //     let p = pat_item.path;
    //     let mut pattern = Self::new(pcx);
    //     let (name, meta_decls, _, _, item_or_patt_op, _) = pat_item.get_matched();
    //     pattern.meta = Arc::new(NonLocalMetaVars::from_meta_decls(
    //         meta_decls.as_ref().map(|meta_decls| with_path(p, meta_decls)),
    //         pcx,
    //         symbol_table,
    //     ));
    //     let name = Symbol::intern(name.span.as_str());
    //     pattern.add_item_or_patt_op(Some(name), with_path(p, item_or_patt_op), symbol_table);
    //     pattern
    // }

    pub fn add_pattern(
        &mut self,
        pat_item: WithPath<'pcx, &'pcx pairs::pattBlockItem<'pcx>>,
        symbol_tables: &'pcx FxHashMap<Symbol, rpl_meta::symbol_table::SymbolTable<'_>>,
    ) {
        let p = pat_item.path;
        let (name, meta_decls, _, _, item_or_patt_op, _) = pat_item.get_matched();
        let name = Symbol::intern(name.span.as_str());
        let symbol_table = symbol_tables.get(&name).unwrap();
        let meta = Arc::new(NonLocalMetaVars::from_meta_decls(
            meta_decls.as_ref().map(|meta_decls| with_path(p, meta_decls)),
            self.pcx,
            symbol_table,
        ));
        self.add_item_or_patt_op(Some(name), with_path(p, item_or_patt_op), symbol_table, meta);
    }

    fn add_item_or_patt_op(
        &mut self,
        pat_name: Option<Symbol>,
        item_or_patt_op: WithPath<'pcx, &'pcx pairs::RustItemOrPatternOperation<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
    ) {
        let p = item_or_patt_op.path;
        match &***item_or_patt_op {
            Choice3::_2(_patt_op) => {
                // FIXME: process the patt operation
                todo!()
            },
            _ => {
                let item = item_or_patt_op.RustItem();
                let items = item_or_patt_op.RustItems();
                let items = if let Some(items) = items {
                    items.get_matched().1.iter_matched().collect::<Vec<_>>()
                } else {
                    // unwrap here is safe because the `RustItem` or `RustItems` is not `None`
                    vec![item.unwrap()]
                };
                for item in items {
                    self.add_item(pat_name, with_path(p, item), meta.clone(), symbol_table);
                }
            },
        }
    }
    fn add_item(
        &mut self,
        pat_name: Option<Symbol>,
        item: WithPath<'pcx, &'pcx pairs::RustItem<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
    ) {
        let p = item.path;
        match &***item {
            Choice4::_0(rust_fn) => {
                let fn_name = Symbol::intern(rust_fn.FnSig().FnName().span.as_str());
                let fn_symbol_table = symbol_table.get_fn(fn_name).unwrap();
                self.add_fn(pat_name, WithPath::new(p, rust_fn), meta, fn_symbol_table);
            },
            Choice4::_1(rust_struct) => self.add_struct(rust_struct),
            Choice4::_2(rust_enum) => self.add_enum(rust_enum),
            Choice4::_3(_rust_impl) => todo!("check impl in meta pass"),
        }
    }

    pub fn add_diag(&mut self, diag: &'pcx pairs::diagBlock<'_>) {
        for item in diag.get_matched().2.iter_matched() {
            let (diag, name) = DynamicErrorBuilder::from_item(item);
            let prev = self.diag.insert(name, diag);
            debug_assert!(prev.is_none(), "Duplicate diagnostic for {:?}", name); //FIXME: raise an error
        }
    }
}

// fn-related methods
impl<'pcx> Pattern<'pcx> {
    fn add_fn(
        &mut self,
        pat_name: Option<Symbol>,
        rust_fn: WithPath<'pcx, &'pcx pairs::Fn<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        fn_symbol_table: &'pcx FnSymbolTable<'pcx>,
    ) {
        let fn_pat = Fn::from(rust_fn, self.pcx, fn_symbol_table, meta);
        let fn_pat = self.pcx.alloc_fn(fn_pat);
        if let Some(pat_name) = pat_name {
            self.fns.fns.insert(pat_name, fn_pat);
        }
        let fn_name = fn_pat.name;
        match fn_name.as_str() {
            "_" => {
                // unnamed function, add it to the unnamed_fns
                self.fns.unnamed_fns.push(fn_pat);
            },
            _ => {
                // named function, add it to the named_fns
                self.fns.named_fns.insert(fn_name, fn_pat);
            },
        }
    }
}

// struct-related methods
impl Pattern<'_> {
    fn add_struct(&mut self, _rust_struct: &pairs::Struct<'_>) {
        todo!()
    }
}

// enum-related methods
impl Pattern<'_> {
    fn add_enum(&mut self, _rust_enum: &pairs::Enum<'_>) {
        todo!()
    }
}
