use std::ops::Deref;

use rpl_parser::generics::{Choice2, Choice4};
use rpl_parser::pairs;
use rustc_span::Symbol;

// Attention:
// When you add a new module here,
// Try to keep all predicate signatures consistent in it.
mod multiple_tys;
mod single_ty;
mod translate;
mod trivial;

pub use multiple_tys::*;
pub use single_ty::*;
pub use translate::*;
pub use trivial::*;

// FIXME: performance
// Attention:
// When you add a new predicate,
// Add it to the list below.
pub const ALL_PREDICATES: &[&str] = &[
    // single_ty_preds
    "is_all_safe_trait",
    "is_not_unpin",
    "is_sync",
    "is_integral",
    "is_ptr",
    "is_primitive",
    "needs_drop",
    // translate_preds
    "translate_from_function",
    // trivial_preds
    "false",
    "true",
    // multiple_tys_preds
    "same_size",
    "same_abi_and_pref_align",
];

#[derive(Clone, Copy, Debug)]
pub enum PredicateKind {
    Ty(SingleTyPredsFnPtr),
    Translate(TranslatePredsFnPtr),
    Trivial(TrivialPredsFnPtr),
    MultipleTys(MultipleTysPredsFnPtr),
}

impl From<&str> for PredicateKind {
    fn from(s: &str) -> Self {
        match s {
            "is_all_safe_trait" => Self::Ty(is_all_safe_trait),
            "is_not_unpin" => Self::Ty(is_not_unpin),
            "is_sync" => Self::Ty(is_sync),
            "is_integral" => Self::Ty(is_integral),
            "is_ptr" => Self::Ty(is_ptr),
            "is_primitive" => Self::Ty(is_primitive),
            "translate_from_function" => Self::Translate(translate_from_function),
            "false" => Self::Trivial(r#false),
            "true" => Self::Trivial(r#true),
            "same_size" => Self::MultipleTys(same_size),
            "same_abi_and_pref_align" => Self::MultipleTys(same_abi_and_pref_align),
            _ => unreachable!("Unknown predicate: {}", s),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct PredicateConjunction {
    pub clauses: Vec<PredicateClause>,
}

pub type Predicate<'pcx> = &'pcx PredicateConjunction;

impl PredicateConjunction {
    pub fn from_pairs(preds: &pairs::PredicateConjunction<'_>) -> Self {
        let (first, following) = preds.get_matched();
        let clauses = std::iter::once(first)
            .chain(following.iter_matched().map(|and_pred| and_pred.get_matched().1))
            .map(|pred| PredicateClause::from_pairs(pred))
            .collect();
        Self { clauses }
    }
}

// PredicateClause is a `||` of PredicateTerms
#[derive(Clone, Default, Debug)]
pub struct PredicateClause {
    pub terms: Vec<PredicateTerm>,
}

impl PredicateClause {
    fn from_pairs(pred: &pairs::PredicateClause<'_>) -> Self {
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
}

#[derive(Clone, Debug)]
pub struct PredicateTerm {
    pub kind: PredicateKind,
    pub args: Vec<PredicateArg>,
    pub is_neg: bool,
}

impl PredicateTerm {
    fn from_pairs(pred: &pairs::PredicateTerm<'_>) -> Self {
        let (pred, is_neg) = match pred.deref() {
            Choice2::_0(pred) => (pred, false),
            Choice2::_1(pred) => (pred.get_matched().1, true),
        };
        let (pred_name, _, args, _) = pred.get_matched();
        let kind = PredicateKind::from(pred_name.span.as_str());
        let args = if let Some(args) = args {
            let (first, following, _) = args.get_matched();
            let following = following
                .iter_matched()
                .map(|comma_with_elem| comma_with_elem.get_matched().1);
            std::iter::once(first)
                .chain(following)
                .map(PredicateArg::from_pairs)
                .collect()
        } else {
            vec![]
        };
        Self { kind, is_neg, args }
    }
}

#[derive(Clone, Debug)]
pub enum PredicateArg {
    Label(Symbol),
    MetaVar(Symbol),
    Path(Vec<Symbol>),
    SelfValue,
}

impl PredicateArg {
    pub fn from_pairs(arg: &pairs::PredicateArg<'_>) -> Self {
        match arg.deref() {
            Choice4::_0(label) => Self::Label(Symbol::intern(label.LabelName().span.as_str())),
            Choice4::_1(meta_var) => Self::MetaVar(Symbol::intern(meta_var.span.as_str())),
            Choice4::_2(path) => Self::Path(path.span.as_str().split("::").map(Symbol::intern).collect()),
            Choice4::_3(_self_value) => Self::SelfValue,
        }
    }
}
