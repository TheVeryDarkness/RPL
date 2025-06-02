#![feature(rustc_private)]
#![feature(let_chains)]
#![feature(if_let_guard)]

extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_fluent_macro;
extern crate rustc_hir;
extern crate rustc_infer;
extern crate rustc_lint_defs;
extern crate rustc_macros;
extern crate rustc_middle;
extern crate rustc_passes;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_trait_selection;
#[macro_use]
extern crate tracing;
extern crate either;

use std::ops::Deref;

use rpl_parser::generics::Choice2;
use rpl_parser::pairs;

use rustc_middle::mir;
use rustc_middle::ty::{self, Ty, TyCtxt};

// Attention:
// When you add a new module here,
// Try to keep all predicate signatures consistent in it.
mod is_ty;
mod trait_bound;
mod translate;
mod trivial;

pub use is_ty::*;
pub use trait_bound::*;
pub use translate::*;
pub use trivial::*;

// FIXME: performance
// Attention:
// When you add a new predicate,
// Add it to the list below.
pub const ALL_PREDICATES: &[&str] = &[
    // trait_bound
    "is_all_safe_trait",
    "is_not_unpin",
    "is_sync",
    // translate
    "translate_from_hir_function",
    // ty
    "is_integral",
    "is_ptr",
    "is_primitive",
    // trivial
    "false",
    "true",
];

#[derive(Clone, Copy, Debug)]
pub enum PredicateKind {
    TraitBound(TraitBoundPredicateTy),
    Translate(TranslatePredicateTy),
    Ty(IsTyPredicateTy),
    Trivial(TrivialPredicate),
}

impl From<&str> for PredicateKind {
    fn from(s: &str) -> Self {
        match s {
            "is_all_safe_trait" => Self::TraitBound(is_all_safe_trait),
            "is_not_unpin" => Self::TraitBound(is_not_unpin),
            "is_sync" => Self::TraitBound(is_sync),
            "translate_from_hir_function" => Self::Translate(translate_from_hir_function),
            "is_integral" => Self::Ty(is_integral),
            "is_ptr" => Self::Ty(is_ptr),
            "is_primitive" => Self::Ty(is_primitive),
            "is_false" => Self::Trivial(is_false),
            "is_true" => Self::Trivial(is_true),
            _ => unreachable!("Unknown predicate: {}", s),
        }
    }
}

impl PredicateKind {
    fn evaluate<'tcx>(
        &self,
        tcx: Option<TyCtxt<'tcx>>,
        typing_env: Option<ty::TypingEnv<'tcx>>,
        ty: Option<Ty<'tcx>>,
        mir_location: Option<mir::Location>,
        hir_fn_path: Option<&str>,
        body: Option<&mir::Body<'tcx>>,
    ) -> bool {
        match self {
            Self::TraitBound(p) => {
                debug_assert!(tcx.is_some(), "tcx is required when evaluating trait_bound predicates");
                debug_assert!(
                    typing_env.is_some(),
                    "typing_env is required when evaluating trait_bound predicates"
                );
                debug_assert!(ty.is_some(), "ty is required when evaluating trait_bound predicates");
                p(tcx.unwrap(), typing_env.unwrap(), ty.unwrap())
            },
            Self::Translate(p) => {
                debug_assert!(
                    mir_location.is_some(),
                    "mir_location is required when evaluating translate predicates"
                );
                debug_assert!(
                    hir_fn_path.is_some(),
                    "hir_fn_path is required when evaluating translate predicates"
                );
                debug_assert!(tcx.is_some(), "tcx is required when evaluating translate predicates");
                debug_assert!(body.is_some(), "body is required when evaluating translate predicates");
                p(mir_location.unwrap(), hir_fn_path.unwrap(), tcx.unwrap(), body.unwrap())
            },
            Self::Ty(p) => {
                debug_assert!(tcx.is_some(), "tcx is required when evaluating ty predicates");
                debug_assert!(
                    typing_env.is_some(),
                    "typing_env is required when evaluating ty predicates"
                );
                debug_assert!(ty.is_some(), "ty is required when evaluating ty predicates");
                p(tcx.unwrap(), typing_env.unwrap(), ty.unwrap())
            },
            Self::Trivial(p) => p(),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct PredicateConjunction {
    clauses: Vec<PredicateClause>,
}

impl PredicateConjunction {
    pub fn from_pairs_opt<'i>(preds: Option<&pairs::PredicateConjunction<'i>>) -> Self {
        if let Some(preds) = preds {
            let (first, following) = preds.get_matched();
            let clauses = std::iter::once(first)
                .chain(following.iter_matched().map(|and_pred| and_pred.get_matched().1))
                .map(|pred| PredicateClause::from_pairs(pred))
                .collect();
            Self { clauses }
        } else {
            Self::default()
        }
    }

    pub fn evaluate<'tcx>(
        &self,
        tcx: Option<TyCtxt<'tcx>>,
        typing_env: Option<ty::TypingEnv<'tcx>>,
        ty: Option<Ty<'tcx>>,
        mir_location: Option<mir::Location>,
        hir_fn_path: Option<&str>,
        body: Option<&mir::Body<'tcx>>,
    ) -> bool {
        self.clauses
            .iter()
            .all(|clause| clause.evaluate(tcx, typing_env, ty, mir_location, hir_fn_path, body))
    }
}

// PredicateClause is a `||` of PredicateTerms
#[derive(Clone, Default, Debug)]
struct PredicateClause {
    terms: Vec<PredicateTerm>,
}

impl PredicateClause {
    fn from_pairs<'i>(pred: &pairs::PredicateClause<'i>) -> Self {
        let terms = match pred.deref() {
            Choice2::_0(pred) => vec![PredicateTerm::from_pairs(pred)],
            Choice2::_1(preds) => {
                let (_, first, following, _) = preds.get_matched();
                std::iter::once(first)
                    .chain(following.iter_matched().map(|or_pred| or_pred.get_matched().1))
                    .map(|pred| PredicateTerm::from_pairs(pred))
                    .collect()
            },
        };
        Self { terms }
    }

    fn evaluate<'tcx>(
        &self,
        tcx: Option<TyCtxt<'tcx>>,
        typing_env: Option<ty::TypingEnv<'tcx>>,
        ty: Option<Ty<'tcx>>,
        mir_location: Option<mir::Location>,
        hir_fn_path: Option<&str>,
        body: Option<&mir::Body<'tcx>>,
    ) -> bool {
        self.terms
            .iter()
            .any(|term| term.evaluate(tcx, typing_env, ty, mir_location, hir_fn_path, body))
    }
}

#[derive(Clone, Debug)]
struct PredicateTerm {
    kind: PredicateKind,
    is_neg: bool,
}

impl PredicateTerm {
    fn from_pairs<'i>(pred: &pairs::PredicateTerm<'i>) -> Self {
        let (pred, is_neg) = match pred.deref() {
            Choice2::_0(pred) => (pred, false),
            Choice2::_1(pred) => (pred.get_matched().1, true),
        };
        let pred_name = pred.get_matched().0.span.as_str();
        let kind = PredicateKind::from(pred_name);
        Self { kind, is_neg }
    }

    fn evaluate<'tcx>(
        &self,
        tcx: Option<TyCtxt<'tcx>>,
        typing_env: Option<ty::TypingEnv<'tcx>>,
        ty: Option<Ty<'tcx>>,
        mir_location: Option<mir::Location>,
        hir_fn_path: Option<&str>,
        body: Option<&mir::Body<'tcx>>,
    ) -> bool {
        let result = self.kind.evaluate(tcx, typing_env, ty, mir_location, hir_fn_path, body);
        if self.is_neg { !result } else { result }
    }
}
