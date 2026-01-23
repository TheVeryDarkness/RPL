use core::fmt;
use std::collections::HashMap;

use rpl_constraints::Const;
use rpl_meta::collect_elems_separated_by_comma;
use rpl_parser::generics::{Choice2, Choice3};
use rpl_parser::pairs;
use rustc_data_structures::fx::FxHashMap;
use rustc_errors::MultiSpan;
use rustc_hir::FnDecl;
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_middle::mir::{Body, PlaceRef};
use rustc_middle::ty::Ty;
use rustc_span::{Span, Symbol};

use super::non_local_meta_vars::{ConstVarIdx, PlaceVarIdx, TyVarIdx};
use crate::pat::NonLocalMetaVars;

pub trait MatchedMetaVars<'tcx>: fmt::Debug {
    /// Get the matched type of the type meta variable at `idx`.
    fn type_meta_var(&self, idx: TyVarIdx) -> Ty<'tcx>;
    /// Get the matched constant of the constant meta variable at `idx`.
    fn const_meta_var(&self, idx: ConstVarIdx) -> Const<'tcx>;
    /// Get the matched place of the place meta variable at `idx`.
    fn place_meta_var(&self, idx: PlaceVarIdx) -> PlaceRef<'tcx>;
}

pub trait Matched<'tcx>: fmt::Debug + MatchedMetaVars<'tcx> {
    /// Get the span of a labeled statement by the name of the label.
    fn span(&self, body: &Body<'tcx>, decl: &FnDecl<'tcx>, name: &str) -> Span;
    /// Get the multi-span of multiple labeled statements by the names of the labels.
    fn multi_span(&self, body: &Body<'tcx>, decl: &FnDecl<'tcx>, name: &[&str]) -> MultiSpan {
        let spans = name.iter().map(|n| self.span(body, decl, n)).collect();
        MultiSpan::from_spans(spans)
    }
}

pub trait MirGraphs<'tcx>: fmt::Debug {
    /// Get the name, the MIR body, and the function declaration of the function with `def_id`.
    fn get_fn(&self, def_id: LocalDefId) -> (Option<Symbol>, &Body<'tcx>, &FnDecl<'tcx>);
}

pub trait MatchedMetaVars2<'tcx>: fmt::Debug {
    /// Get the matched type of the type meta variable at `idx`.
    fn type_meta_var(&self, idx: TyVarIdx) -> Ty<'tcx>;
    /// Get the matched constant of the constant meta variable at `idx`.
    fn const_meta_var(&self, idx: ConstVarIdx) -> Const<'tcx>;
    /// Get the matched place of the place meta variable at `idx`.
    fn place_meta_var(&self, idx: PlaceVarIdx) -> (LocalDefId, PlaceRef<'tcx>);
}

pub trait Matched2<'tcx>: fmt::Debug + MatchedMetaVars2<'tcx> {
    /// Get the span of a labeled statement by the name of the label.
    fn span(&self, fns: &impl MirGraphs<'tcx>, name: &str) -> Span;
    /// Get the multi-span of multiple labeled statements by the names of the labels.
    fn multi_span(&self, fns: &impl MirGraphs<'tcx>, names: &[&str]) -> MultiSpan {
        let spans = names.iter().map(|n| self.span(fns, n)).collect();
        MultiSpan::from_spans(spans)
    }
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
}
