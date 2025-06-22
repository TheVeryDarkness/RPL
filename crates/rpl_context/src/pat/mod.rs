use std::ops::Deref;
use std::sync::Arc;

use error::{DynamicError, DynamicErrorBuilder};
use rpl_constraints::{Constraint, predicates};
use rpl_meta::check::Inline;
use rpl_meta::collect_elems_separated_by_comma;
use rpl_meta::symbol_table::WithPath;
use rpl_meta::utils::Ident;
use rpl_parser::generics::{Choice2, Choice3, Choice4};
use rpl_parser::pairs;
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::FnDecl;
use rustc_middle::mir::Body;
use rustc_span::Symbol;

use crate::PatCtxt;

mod error;
mod item;
mod matched;
mod mir;
mod non_local_meta_vars;
mod pretty;
mod ty;
mod utils;

pub use item::*;
pub use matched::{Matched, MatchedMap};
pub use mir::*;
pub use non_local_meta_vars::*;
pub use ty::*;

pub type Label = Symbol;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Spanned {
    Location(mir::Location),
    Local(mir::Local),
    Body,
    Output,
}

pub type LabelMap = FxHashMap<Label, Spanned>;

#[derive(Debug, Clone, Copy)]
pub enum PattOrUtil {
    Patt,
    Util,
}

#[derive(Default)]
pub(crate) struct PatAttr {
    diag: Option<Symbol>,
}

impl PatAttr {
    fn parse(pairs: &pairs::Attr<'_>) -> Self {
        let mut result = Self::default();
        let (_, _, attrs, _) = pairs.get_matched();
        for attr in collect_elems_separated_by_comma!(attrs) {
            let (key, value) = attr.get_matched();
            match key.span.as_str() {
                "diag" => match value {
                    Some(Choice2::_0(_)) | None => unreachable!(),
                    Some(Choice2::_1(msg)) => {
                        let (_, msg) = msg.get_matched();
                        result.diag = Some(Symbol::intern(msg.diagMessageInner().span.as_str()));
                    },
                },
                _ => unreachable!(),
            }
        }
        result
    }
}

#[derive(Default, Debug)]
pub struct FnAttr {
    output: Option<Symbol>,
    inline: Option<Inline>,
    predicates: Vec<predicates::SingleFnPredsFnPtr>,
}

impl FnAttr {
    fn parse<'i>(pairs: impl Iterator<Item = &'i pairs::Attr<'i>>) -> Self {
        let mut result = Self::default();
        for pairs in pairs {
            let (_, _, attrs, _) = pairs.get_matched();
            for attr in collect_elems_separated_by_comma!(attrs) {
                let (key, value) = attr.get_matched();
                match key.span.as_str() {
                    "output" => match value {
                        Some(Choice2::_0(_)) | None => unreachable!(),
                        Some(Choice2::_1(msg)) => {
                            let (_, msg) = msg.get_matched();
                            result.output = Some(Symbol::intern(msg.diagMessageInner().span.as_str()));
                        },
                    },
                    "inline" => match value {
                        Some(Choice2::_1(_)) | None => unreachable!(),
                        Some(Choice2::_0(msg)) => {
                            let (_, msg, _) = msg.get_matched();
                            if let Some(msg) = msg {
                                let inner = collect_elems_separated_by_comma!(msg).collect::<Vec<_>>();
                                if inner.len() != 1 {
                                    panic!("Expected exactly one output, found {}", inner.len());
                                }
                                let (level, attr) = inner[0].get_matched();
                                assert!(attr.is_none(), "Unexpected attribute in output: {attr:?}");
                                result.inline = match level.span.as_str() {
                                    "always" => Some(Inline::Always),
                                    "any" => Some(Inline::Any),
                                    "never" => Some(Inline::Never),
                                    _ => panic!("Unexpected inline level: {}", level.span.as_str()),
                                };
                            } else {
                                result.inline = Some(Inline::Normal);
                            }
                        },
                    },
                    "rpl" => match value {
                        Some(Choice2::_1(_)) | None => unreachable!(),
                        Some(Choice2::_0(inner)) => {
                            let (_, inner, _) = inner.get_matched();
                            if let Some(inner) = inner {
                                for inner in collect_elems_separated_by_comma!(inner) {
                                    let (key, value) = inner.get_matched();
                                    assert!(value.is_none(), "Unexpected value in RPL attribute: {value:?}");
                                    match key.span.as_str() {
                                        "requires_monomorphization" => {
                                            result.predicates.push(predicates::requires_monomorphization);
                                        },
                                        _ => panic!("Unexpected RPL attribute: {}", key.span.as_str()),
                                    }
                                }
                            }
                        },
                    },
                    _ => unreachable!(),
                }
            }
        }
        result
    }
}

pub enum PatternItem<'pcx> {
    RustItems(RustItems<'pcx>),
    RPLPatternOperation(PatternOperation<'pcx>),
}

impl PatternItem<'_> {
    pub fn meta(&self) -> &NonLocalMetaVars<'_> {
        match self {
            PatternItem::RustItems(items) => &items.meta,
            PatternItem::RPLPatternOperation(op) => &op.meta,
        }
    }
    pub(crate) fn diag_name(&self) -> Option<Symbol> {
        match self {
            PatternItem::RustItems(items) => items.diag,
            PatternItem::RPLPatternOperation(op) => op.diag,
        }
    }
}

pub struct RustItems<'pcx> {
    pub pcx: PatCtxt<'pcx>,
    pub meta: Arc<NonLocalMetaVars<'pcx>>,
    pub adts: FxHashMap<Symbol, Adt<'pcx>>,
    pub fns: FnPatterns<'pcx>,
    pub impls: FxHashMap<Symbol, Impl<'pcx>>,
    pub diag: Option<Symbol>,
}

impl<'pcx> RustItems<'pcx> {
    pub(crate) fn new(pcx: PatCtxt<'pcx>, meta: Arc<NonLocalMetaVars<'pcx>>, attr: PatAttr) -> Self {
        Self {
            pcx,
            meta,
            adts: Default::default(),
            fns: Default::default(),
            impls: Default::default(),
            diag: attr.diag,
        }
    }

    fn add_item(
        &mut self,
        pat_name: Option<Symbol>,
        item: WithPath<'pcx, &'pcx pairs::RustItemWithConstraint<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
    ) {
        let path = item.path;
        let (attr, item, where_block) = item.get_matched();
        let constraints = Constraint::from_where_block_opt(where_block, path).expect("unexpected error in constraints");
        match item.deref() {
            Choice4::_0(rust_fn) => {
                let fn_name = Symbol::intern(rust_fn.FnSig().FnName().span.as_str());
                let fn_symbol_table = symbol_table.get_fn(fn_name).unwrap();
                let attr = FnAttr::parse(attr.iter_matched());
                self.add_fn(WithPath::new(path, rust_fn), meta, fn_symbol_table, attr, constraints);
            },
            Choice4::_1(rust_struct) => {
                debug_assert!(attr.content.is_empty(), "Unhandled attributes in struct: {:?}", attr);
                self.add_struct(pat_name, with_path(path, rust_struct), meta, symbol_table, constraints)
            },
            Choice4::_2(rust_enum) => {
                debug_assert!(attr.content.is_empty(), "Unhandled attributes in enum: {:?}", attr);
                self.add_enum(pat_name, with_path(path, rust_enum), meta, symbol_table, constraints)
            },
            Choice4::_3(rust_impl) => {
                debug_assert!(attr.content.is_empty(), "Unhandled attributes in impl: {:?}", attr);
                self.add_impl(pat_name, with_path(path, rust_impl), meta, symbol_table, constraints)
            },
        }
    }

    #[instrument(level = "debug", skip(self, rust_fn, meta, fn_symbol_table))]
    fn add_fn(
        &mut self,
        rust_fn: WithPath<'pcx, &'pcx pairs::Fn<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        fn_symbol_table: &'pcx FnSymbolTable<'pcx>,
        attr: FnAttr,
        constraints: Vec<Constraint>,
    ) {
        let fn_pat = FnPattern::from(rust_fn, self.pcx, fn_symbol_table, meta, attr, constraints);
        let fn_pat = self.pcx.alloc_fn(fn_pat);
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

    #[instrument(level = "debug", skip(self, rust_struct, symbol_table))]
    fn add_struct(
        &mut self,
        pat_name: Option<Symbol>,
        rust_struct: WithPath<'pcx, &'pcx pairs::Struct<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'pcx>,
        constraints: Vec<Constraint>,
    ) {
        let mut struct_inner = StructInner::default();
        let name = rust_struct.MetaVariable();
        if let Some(fields) = rust_struct.get_matched().4 {
            let fields = collect_elems_separated_by_comma!(fields);
            for field in fields {
                let (name, _, ty) = field.get_matched();
                let name = Symbol::intern(name.span.as_str());
                let ty = Ty::from(with_path(rust_struct.path, ty), self.pcx, symbol_table);
                let field = Field { ty };
                struct_inner.fields.insert(name, field);
            }
        }

        let struct_pat = Adt::new_struct(struct_inner, meta, constraints);
        // let struct_pat = self.pcx.alloc_struct(struct_pat);
        self.adts.insert(Symbol::intern(name.span.as_str()), struct_pat);
    }

    #[instrument(level = "debug", skip(self, rust_enum, symbol_table))]
    fn add_enum(
        &mut self,
        pat_name: Option<Symbol>,
        rust_enum: WithPath<'pcx, &'pcx pairs::Enum<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'pcx>,
        constraints: Vec<Constraint>,
    ) {
        let mut enum_inner = EnumInner::default();
        let name = rust_enum.MetaVariable();

        if let Some(variants) = rust_enum.EnumVariantsSeparatedByComma() {
            let variants = collect_elems_separated_by_comma!(variants);
            for variant in variants {
                let mut enum_variant = Variant::default();
                let identifier = match variant.deref() {
                    Choice3::_0(variant) => {
                        if let Some(fields) = variant.get_matched().2 {
                            let fields = collect_elems_separated_by_comma!(fields);
                            for field in fields {
                                let (name, _, ty) = field.get_matched();
                                let name = Symbol::intern(name.span.as_str());
                                let ty = Ty::from(with_path(rust_enum.path, ty), self.pcx, symbol_table);
                                let field = Field { ty };
                                enum_variant.fields.insert(name, field);
                            }
                        }
                        variant.get_matched().0
                    },
                    Choice3::_1(variant) => {
                        let (name, _, ty, _) = variant.get_matched();
                        let name = Symbol::intern(name.span.as_str());
                        let ty = Ty::from(with_path(rust_enum.path, ty), self.pcx, symbol_table);
                        let field = Field { ty };
                        enum_variant.fields.insert(name, field);
                        variant.get_matched().0
                    },
                    Choice3::_2(unit) => unit,
                };
                let ident = Ident::from(identifier);
                enum_inner.insert(ident.name, enum_variant);
            }
        }

        let enum_pat = Adt::new_enum(enum_inner, meta, constraints);
        // let struct_pat = self.pcx.alloc_struct(struct_pat);
        self.adts.insert(Symbol::intern(name.span.as_str()), enum_pat);
    }

    #[instrument(level = "debug", skip(self, rust_impl, meta, symbol_table))]
    fn add_impl(
        &mut self,
        pat_name: Option<Symbol>,
        rust_impl: WithPath<'pcx, &'pcx pairs::Impl<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'pcx>,
        constraints: Vec<Constraint>,
    ) {
        let p = rust_impl.path;
        let (_, impl_kind, ty, _, fns, _) = rust_impl.get_matched();
        let impl_sym_tab = symbol_table.get_impl(ty, impl_kind.as_ref()).unwrap();
        let ty = Ty::from(WithPath::new(p, ty), self.pcx, symbol_table);
        let trait_id = impl_kind
            .as_ref()
            .map(|impl_kind| Path::from_pairs(impl_kind.get_matched().0, self.pcx));
        let fns = fns
            .iter_matched()
            .map(|rust_fn| {
                let (rust_fn, where_block) = rust_fn.get_matched();
                let constraints =
                    Constraint::from_where_block_opt(where_block, p).expect("unexpected error in constraints");
                let fn_name = Symbol::intern(rust_fn.FnSig().FnName().span.as_str());
                let fn_sym_tab = impl_sym_tab.inner.get_fn(fn_name).unwrap();
                let fn_def = FnPattern::from(
                    WithPath::new(p, rust_fn),
                    self.pcx,
                    fn_sym_tab,
                    Arc::clone(&meta),
                    FnAttr::default(),
                    constraints,
                );
                (fn_name, fn_def)
            })
            .collect();
        let impl_pat = Impl {
            meta,
            ty,
            trait_id,
            fns,
            constraints,
        };
        debug!(ty = ?impl_pat.ty, trait_id = ?impl_pat.trait_id, fns = ?impl_pat.fns.keys());
        if let Some(pat_name) = pat_name {
            self.impls.insert(pat_name, impl_pat);
        }
    }

    #[instrument(level = "trace", skip(self), fields(adts = ?self.adts.keys()), ret)]
    pub fn get_adt(&self, adt: Symbol) -> Option<&Adt<'pcx>> {
        self.adts.get(&adt)
    }
}

/// `positive` is a list of positive pattern items, `negative` is a list of negative pattern items,
/// they are joined together to form a pattern operation.
///
/// `(positive_1 | positive_2 | ... | positive_n) & !(negative_1 | negative_2 | ... | negative_m)`
pub struct PatternOperation<'pcx> {
    pub pcx: PatCtxt<'pcx>,
    pub meta: Arc<NonLocalMetaVars<'pcx>>,
    pub positive: (&'pcx PatternItem<'pcx>, MatchedMap),
    pub negative: Vec<(&'pcx PatternItem<'pcx>, MatchedMap)>,
    pub diag: Option<Symbol>,
}

/// Corresponds to a pattern file in RPL, not a pattern item.
pub struct Pattern<'pcx> {
    pub pcx: PatCtxt<'pcx>,
    pub patt_block: FxHashMap<Symbol, PatternItem<'pcx>>, // indexed by pat_name
    pub util_block: FxHashMap<Symbol, &'pcx PatternItem<'pcx>>, // indexed by pat_name
    diag_block: FxHashMap<Symbol, DynamicErrorBuilder<'pcx>>,
}

impl<'pcx> Pattern<'pcx> {
    pub(crate) fn new(pcx: PatCtxt<'pcx>) -> Self {
        Self {
            pcx,
            patt_block: Default::default(),
            util_block: Default::default(),
            diag_block: Default::default(),
        }
    }

    pub fn get_diag<'tcx>(
        &self,
        pat_name: Symbol,
        body: &Body<'tcx>,
        decl: &FnDecl<'tcx>,
        matched: &impl Matched<'tcx>,
    ) -> Result<Box<DynamicError>, Box<DynamicError>> {
        Ok(Box::new(
            self.diag_block
                .get(&pat_name)
                .ok_or_else(|| Box::new(DynamicError::default_diagnostic(pat_name, body.span)))?
                .build(body, decl, matched),
        ))
    }
}

impl<'pcx> Pattern<'pcx> {
    pub fn add_pattern_item(
        &mut self,
        pat_item: WithPath<'pcx, &'pcx pairs::RPLPatternItem<'pcx>>,
        symbol_tables: &'pcx FxHashMap<Symbol, rpl_meta::symbol_table::SymbolTable<'_>>,
        block_type: PattOrUtil,
    ) {
        let p = pat_item.path;
        let (attr, name, meta_decls, _, item_or_patt_op) = pat_item.get_matched();
        let name = Symbol::intern(name.span.as_str());
        let symbol_table = symbol_tables.get(&name).unwrap();
        let meta = Arc::new(NonLocalMetaVars::from_meta_decls(
            meta_decls.as_ref().map(|meta_decls| with_path(p, meta_decls)),
            self.pcx,
            symbol_table,
        ));
        self.add_item_or_patt_op(
            name,
            attr.as_ref(),
            with_path(p, item_or_patt_op),
            symbol_table,
            meta,
            block_type,
        );
    }

    #[instrument(level = "debug", skip(self, item_or_patt_op, symbol_table, meta), fields(patt_block = ?self.patt_block.keys(), util_block = ?self.util_block.keys()))]
    fn add_item_or_patt_op(
        &mut self,
        pat_name: Symbol,
        attr: Option<&'pcx pairs::Attr<'pcx>>,
        item_or_patt_op: WithPath<'pcx, &'pcx pairs::RustItemsOrPatternOperation<'pcx>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        block_type: PattOrUtil,
    ) {
        let p = item_or_patt_op.path;
        match &***item_or_patt_op {
            Choice3::_0(item) => {
                self.add_items(
                    pat_name,
                    attr,
                    with_path(p, std::iter::once(item)),
                    symbol_table,
                    meta,
                    block_type,
                );
            },
            Choice3::_1(items) => {
                self.add_items(
                    pat_name,
                    attr,
                    with_path(p, items.get_matched().1.iter_matched()),
                    symbol_table,
                    meta,
                    block_type,
                );
            },
            Choice3::_2(patt_op) => {
                self.add_patt_op(pat_name, attr, with_path(p, patt_op), meta, block_type);
            },
        }
    }

    fn patt_op(
        &self,
        meta: &NonLocalMetaVars<'pcx>,
        pat_cfg: &'pcx pairs::PatternConfiguration<'pcx>,
    ) -> (&'pcx PatternItem<'pcx>, MatchedMap) {
        let item = *self.util_block.get(&Ident::from(pat_cfg.Identifier()).name).unwrap();
        let map = MatchedMap::new(meta, item.meta(), pat_cfg.MetaVariableAssignList());
        (item, map)
    }

    #[instrument(level = "debug", skip(self, patt_op, meta), fields(patt_block = ?self.patt_block.keys(), util_block = ?self.util_block.keys()))]
    fn add_patt_op(
        &mut self,
        pat_name: Symbol,
        attr: Option<&'pcx pairs::Attr<'pcx>>,
        patt_op: WithPath<'pcx, &'pcx pairs::PatternOperation<'pcx>>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        block_type: PattOrUtil,
    ) {
        let (pos, neg) = patt_op.PatternConfiguration();
        let positive = self.patt_op(&meta, pos);
        let negative = neg.iter().map(|negative| self.patt_op(&meta, negative)).collect();
        let attr = attr.map(PatAttr::parse).unwrap_or_default();
        let pat_ops = PatternOperation {
            pcx: self.pcx,
            meta,
            positive,
            negative,
            diag: attr.diag,
        };
        match block_type {
            PattOrUtil::Patt => {
                self.patt_block
                    .entry(pat_name)
                    .or_insert(PatternItem::RPLPatternOperation(pat_ops));
            },
            PattOrUtil::Util => {
                self.util_block
                    .entry(pat_name)
                    .or_insert_with(|| self.pcx.alloc_pattern_item(PatternItem::RPLPatternOperation(pat_ops)));
            },
        };
    }

    #[instrument(level = "debug", skip(self, items, symbol_table, meta))]
    fn add_items(
        &mut self,
        pat_name: Symbol,
        attr: Option<&'pcx pairs::Attr<'pcx>>,
        items: WithPath<'pcx, impl Iterator<Item = &'pcx pairs::RustItemWithConstraint<'pcx>>>,
        symbol_table: &'pcx rpl_meta::symbol_table::SymbolTable<'_>,
        meta: Arc<NonLocalMetaVars<'pcx>>,
        block_type: PattOrUtil,
    ) {
        let p = items.path;
        match block_type {
            PattOrUtil::Patt => {
                self.patt_block.entry(pat_name).or_insert_with(|| {
                    let attr = attr.map(PatAttr::parse).unwrap_or_default();
                    let mut rpl_rust_items = RustItems::new(self.pcx, meta.clone(), attr);
                    for item in items.inner {
                        rpl_rust_items.add_item(Some(pat_name), with_path(p, item), meta.clone(), symbol_table);
                    }
                    PatternItem::RustItems(rpl_rust_items)
                });
            },
            PattOrUtil::Util => {
                self.util_block.entry(pat_name).or_insert_with(|| {
                    let attr = attr.map(PatAttr::parse).unwrap_or_default();
                    let mut rpl_rust_items = RustItems::new(self.pcx, meta.clone(), attr);
                    for item in items.inner {
                        rpl_rust_items.add_item(Some(pat_name), with_path(p, item), meta.clone(), symbol_table);
                    }
                    self.pcx.alloc_pattern_item(PatternItem::RustItems(rpl_rust_items))
                });
            },
        };
    }

    pub fn add_diag(
        &mut self,
        diag: WithPath<'pcx, &'pcx pairs::diagBlock<'_>>,
        symbol_tables: &'pcx FxHashMap<Symbol, rpl_meta::symbol_table::SymbolTable<'_>>,
    ) {
        let mut items = FxHashMap::default();
        for item in diag.get_matched().2.iter_matched() {
            let (ident, _, _, _, _, _) = item.get_matched();
            let name = Symbol::intern(ident.span.as_str());
            let prev = items.insert(name, item);
            debug_assert!(prev.is_none(), "Duplicate diagnostic for {:?}", name); //FIXME: raise an error
        }

        for (name, pat_item) in &self.patt_block {
            let symbol_table = symbol_tables.get(&name).unwrap();

            let diag_name = pat_item.diag_name().unwrap_or(*name);
            if let Some(diag_item) = items.get(&diag_name) {
                let diag = DynamicErrorBuilder::from_item(WithPath::new(diag.path, diag_item), &symbol_table.meta_vars)
                    .unwrap_or_else(|err| panic!("{err}"));
                let prev = self.diag_block.insert(*name, diag);
                debug_assert!(prev.is_none(), "Duplicate diagnostic for {:?}", name); //FIXME: raise an error
            } else {
                warn!("No diagnostic found for pattern item {:?} ({:?})", name, diag_name);
            }
        }
    }
}
