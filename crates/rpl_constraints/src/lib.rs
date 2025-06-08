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

pub mod attributes;
pub mod predicates;
pub mod tribool;

#[derive(Debug, Clone)]
pub enum Constraint {
    Pred(predicates::PredicateConjunction),
    Attr(attributes::Attribute),
}

impl Constraint {
    pub fn from_pairs(constraint: &pairs::Constraint<'_>) -> Self {
        match constraint.deref() {
            Choice2::_0(attr) => {
                let attr = attributes::Attribute::from_pairs(attr);
                Constraint::Attr(attr)
            },
            Choice2::_1(preds) => {
                let pred = predicates::PredicateConjunction::from_pairs(preds);
                Constraint::Pred(pred)
            },
        }
    }

    pub fn from_where_block_opt(where_block: &Option<pairs::WhereBlock<'_>>) -> Vec<Self> {
        if let Some(where_block) = where_block
            && let Some(constraints) = where_block.ConstraintsSeparatedByComma()
        {
            let (first, following, _) = constraints.get_matched();
            let following = following
                .iter_matched()
                .map(|comma_with_elem| comma_with_elem.get_matched().1);
            std::iter::once(first).chain(following).map(Self::from_pairs).collect()
        } else {
            Vec::new()
        }
    }
}
