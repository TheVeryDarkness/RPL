use rustc_data_structures::fx::FxHashMap;
use rustc_middle::mir;
use rustc_span::Symbol;

use super::{MetaVars, MirPattern, Path, Ty};

pub struct Adt<'pcx> {
    pub meta: MetaVars<'pcx>,
    pub kind: AdtKind<'pcx>,
}

pub enum AdtKind<'pcx> {
    Struct(Variant<'pcx>),
    Enum(FxHashMap<Symbol, Variant<'pcx>>),
}

#[derive(Default)]
pub struct Variant<'pcx> {
    pub fields: FxHashMap<Symbol, Field<'pcx>>,
}

pub struct Field<'pcx> {
    pub ty: Ty<'pcx>,
}

pub struct Impl<'pcx> {
    pub meta: MetaVars<'pcx>,
    #[expect(dead_code)]
    ty: Ty<'pcx>,
    #[expect(dead_code)]
    trait_id: Option<Path<'pcx>>,
    #[expect(dead_code)]
    fns: FxHashMap<Symbol, Fn<'pcx>>,
}

#[derive(Default)]
pub struct Fns<'pcx> {
    fns: FxHashMap<Symbol, Fn<'pcx>>,
    fn_pats: FxHashMap<Symbol, Fn<'pcx>>,
    unnamed_fns: Vec<Fn<'pcx>>,
}

pub struct Fn<'pcx> {
    pub meta: MetaVars<'pcx>,
    pub params: Params<'pcx>,
    pub ret: Ty<'pcx>,
    pub body: Option<FnBody<'pcx>>,
}

#[derive(Default)]
pub struct Params<'pcx> {
    params: Vec<Param<'pcx>>,
    non_exhaustive: bool,
}

pub struct Param<'pcx> {
    pub mutability: mir::Mutability,
    pub ident: Symbol,
    pub ty: Ty<'pcx>,
}

pub struct TraitDef {}

#[derive(Clone, Copy)]
pub enum FnBody<'pcx> {
    Mir(&'pcx MirPattern<'pcx>),
}

impl<'pcx> Adt<'pcx> {
    pub(crate) fn new_struct() -> Self {
        Self {
            meta: MetaVars::default(),
            kind: AdtKind::Struct(Default::default()),
        }
    }
    pub(crate) fn new_enum() -> Self {
        Self {
            meta: MetaVars::default(),
            kind: AdtKind::Enum(Default::default()),
        }
    }
    pub fn non_enum_variant_mut(&mut self) -> &mut Variant<'pcx> {
        match &mut self.kind {
            AdtKind::Struct(variant) => variant,
            AdtKind::Enum(_) => panic!("cannot mutate non-enum variant of enum"),
        }
    }
    pub fn add_variant(&mut self, name: Symbol) -> &mut Variant<'pcx> {
        match &mut self.kind {
            AdtKind::Struct(_) => panic!("cannot add variant to struct"),
            AdtKind::Enum(variants) => variants.entry(name).or_insert_with(Variant::default),
        }
    }
    pub fn non_enum_variant(&self) -> &Variant<'pcx> {
        match &self.kind {
            AdtKind::Struct(variant) => variant,
            AdtKind::Enum(_) => panic!("cannot access non-enum variant of enum"),
        }
    }
}

impl<'pcx> Variant<'pcx> {
    pub fn add_field(&mut self, name: Symbol, ty: Ty<'pcx>) {
        self.fields.insert(name, Field { ty });
    }
}

impl<'pcx> Fns<'pcx> {
    pub fn get_fn_pat(&self, name: Symbol) -> Option<&Fn<'pcx>> {
        self.fn_pats.get(&name)
    }
    pub fn new_fn(&mut self, name: Symbol, ret: Ty<'pcx>) -> &mut Fn<'pcx> {
        self.fns.entry(name).or_insert_with(|| Fn::new(ret))
    }
    pub fn new_fn_pat(&mut self, name: Symbol, ret: Ty<'pcx>) -> &mut Fn<'pcx> {
        self.fn_pats.entry(name).or_insert_with(|| Fn::new(ret))
    }
    pub fn new_unnamed(&mut self, ret: Ty<'pcx>) -> &mut Fn<'pcx> {
        self.unnamed_fns.push(Fn::new(ret));
        self.unnamed_fns.last_mut().unwrap()
    }
}

impl<'pcx> Fn<'pcx> {
    pub(crate) fn new(ret: Ty<'pcx>) -> Self {
        Self {
            meta: MetaVars::default(),
            params: Params::default(),
            ret,
            body: None,
        }
    }
    pub fn set_body(&mut self, body: FnBody<'pcx>) {
        self.body = Some(body);
    }
    // FIXME: remove this when all kinds of patterns are implemented
    pub fn expect_mir_body(&self) -> &'pcx MirPattern<'pcx> {
        match self.body {
            Some(FnBody::Mir(mir_body)) => mir_body,
            _ => panic!("expected MIR body"),
        }
    }
}

impl<'pcx> Params<'pcx> {
    pub fn add_param(&mut self, ident: Symbol, mutability: mir::Mutability, ty: Ty<'pcx>) {
        self.params.push(Param { mutability, ident, ty });
    }
    pub fn set_non_exhaustive(&mut self) {
        self.non_exhaustive = true;
    }
}
