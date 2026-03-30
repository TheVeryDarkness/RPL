use core::fmt;
use std::collections::HashMap;

use either::Either;
use rpl_constraints::Const;
use rpl_meta::collect_elems_separated_by_comma;
use rpl_parser::generics::{Choice2, Choice3};
use rpl_parser::pairs;
use rustc_data_structures::fx::FxHashMap;
use rustc_errors::MultiSpan;
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_middle::mir::{Body, Local, Location, PlaceRef};
use rustc_middle::ty::Ty;
use rustc_span::{Span, Symbol};

use super::non_local_meta_vars::{ConstVarIdx, PlaceVarIdx, TyVarIdx};
use crate::pat::{self, NonLocalMetaVars};

/// Get matched results of meta variables.
pub trait MatchedMetaVars<'tcx>: fmt::Debug {
    /// Get the matched type of the type meta variable at `idx`.
    fn type_meta_var(&self, idx: TyVarIdx) -> Ty<'tcx>;
    /// Get the matched constant of the constant meta variable at `idx`.
    fn const_meta_var(&self, idx: ConstVarIdx) -> Const<'tcx>;
    /// Get the matched place of the place meta variable at `idx`.
    fn place_meta_var(&self, idx: PlaceVarIdx, bottom: LocalDefId) -> (LocalDefId, PlaceRef<'tcx>);
}

pub trait MatchedLocalVars<'tcx>: fmt::Debug {
    /// Get the matched local of the local meta variable at `idx`.
    fn local(&self, idx: pat::Local) -> Local;
    /// Get the matched location of the local meta variable at `idx`.
    fn location(&self, idx: pat::Location) -> Either<Local, Location>;
}

pub trait Matched<'a, 'tcx, Cx: Copy>: fmt::Debug + MatchedMetaVars<'tcx> {
    /// Get the span that covers the bottom function.
    fn bottom_span(&self, cx: Cx) -> Span;
    /// Get the name of the bottom function.
    fn bottom_name(&self, cx: Cx) -> Option<Symbol>;
    /// Get the span of a labeled statement by the name of the label.
    fn span(&self, cx: Cx, name: &str) -> Span;
    /// Get the multi-span of multiple labeled statements by the names of the labels.
    fn multi_span(&self, cx: Cx, name: &[&str]) -> MultiSpan {
        let spans = name.iter().map(|n| self.span(cx, n)).collect();
        MultiSpan::from_spans(spans)
    }
}

pub trait MirGraphs<'tcx>: fmt::Debug {
    /// Get the name, the MIR body, and the function declaration of the function with `def_id`.
    fn get_fn(&self, def_id: LocalDefId) -> (Option<Symbol>, &Body<'tcx>, &FnDecl<'tcx>);
}

impl<'tcx> MirGraphs<'tcx> for FxHashMap<LocalDefId, (Option<Symbol>, &Body<'tcx>, &FnDecl<'tcx>)> {
    fn get_fn(&self, def_id: LocalDefId) -> (Option<Symbol>, &Body<'tcx>, &FnDecl<'tcx>) {
        self[&def_id]
    }
}

/// - Key: indices/names in destination
/// - Value: indices/names in source
#[derive(Debug, PartialEq, Eq)]
pub struct MatchedMap {
    pub ty_vars: IndexVec<TyVarIdx, TyVarIdx>,
    pub const_vars: IndexVec<ConstVarIdx, ConstVarIdx>,
    pub place_vars: IndexVec<PlaceVarIdx, PlaceVarIdx>,
    pub labels: HashMap<Symbol, Symbol>,
}

impl MatchedMap {
    #[instrument(level = "trace", skip(configuration), ret)]
    pub fn new(
        target: &NonLocalMetaVars<'_>,
        source: &NonLocalMetaVars<'_>,
        configuration: Option<&pairs::MetaVariableAssignsSeparatedByComma<'_>>,
    ) -> Self {
        let mut vars: HashMap<Symbol, Symbol> = HashMap::new();
        let mut labels: HashMap<Symbol, Symbol> = HashMap::new();

        if let Some(configuration) = configuration {
            let assigns = collect_elems_separated_by_comma!(configuration);
            for assign in assigns {
                match &**assign {
                    Choice2::_0(assign) => {
                        let (source_var, _, target_var) = assign.get_matched();
                        match target_var {
                            Choice3::_0(target_label) => {
                                if target_label.span.as_str() != "_" {
                                    todo!()
                                }
                            },
                            Choice3::_1(target_var) => {
                                let target_var = Symbol::intern(target_var.span.as_str());
                                let source_var = Symbol::intern(source_var.span.as_str());
                                vars.try_insert(target_var, source_var).unwrap();
                            },
                            Choice3::_2(_) => todo!(),
                        }
                    },
                    Choice2::_1(assign) => {
                        let (source_label, _, target_label) = assign.get_matched();
                        let target_label = Symbol::intern(target_label.LabelName().span.as_str());
                        let source_label = Symbol::intern(source_label.LabelName().span.as_str());
                        labels.try_insert(source_label, target_label).unwrap();
                    },
                }
            }
        }
        trace!(vars = ?vars, labels = ?labels);
        MatchedMap {
            ty_vars: target
                .ty_vars
                .iter_enumerated()
                .map(|(idx, var)| {
                    source
                        .ty_vars
                        .iter()
                        .find_map(|source_var| {
                            (&source_var.name == vars.get(&var.name).unwrap_or(&var.name)).then_some(idx)
                        })
                        .unwrap()
                })
                .collect(),
            const_vars: target
                .const_vars
                .iter_enumerated()
                .map(|(idx, var)| {
                    source
                        .const_vars
                        .iter()
                        .find_map(|source_var| {
                            (&source_var.name == vars.get(&var.name).unwrap_or(&var.name)).then_some(idx)
                        })
                        .unwrap()
                })
                .collect(),
            place_vars: target
                .place_vars
                .iter_enumerated()
                .map(|(idx, var)| {
                    source
                        .place_vars
                        .iter()
                        .find_map(|source_var| {
                            (&source_var.name == vars.get(&var.name).unwrap_or(&var.name)).then_some(idx)
                        })
                        .unwrap()
                })
                .collect(),
            labels,
        }
    }
    pub fn map_ty_vars<T: Clone>(&self, ty_vars: &IndexVec<TyVarIdx, T>) -> IndexVec<TyVarIdx, T> {
        IndexVec::from_fn_n(|i| ty_vars[self.ty_vars[i]].clone(), ty_vars.len())
    }
    pub fn map_const_vars<T: Clone>(&self, const_vars: &IndexVec<ConstVarIdx, T>) -> IndexVec<ConstVarIdx, T> {
        IndexVec::from_fn_n(|i| const_vars[self.const_vars[i]].clone(), const_vars.len())
    }
    pub fn map_place_vars<T: Clone>(&self, place_vars: &IndexVec<PlaceVarIdx, T>) -> IndexVec<PlaceVarIdx, T> {
        IndexVec::from_fn_n(|i| place_vars[self.place_vars[i]].clone(), place_vars.len())
    }
}
