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

use crate::predicates::PredicateError;

pub mod attributes;
pub mod predicates;
pub mod tribool;

#[derive(Debug, Clone)]
pub enum Constraint {
    Pred(predicates::PredicateConjunction),
    Attr(attributes::Attribute),
}

impl Constraint {
    pub fn from_pairs<'i>(
        constraint: &pairs::Constraint<'i>,
        path: &'i std::path::Path,
    ) -> Result<Self, PredicateError<'i>> {
        match constraint.deref() {
            Choice2::_0(attr) => {
                let attr = attributes::Attribute::from_pairs(attr);
                Ok(Constraint::Attr(attr))
            },
            Choice2::_1(preds) => {
                let pred = predicates::PredicateConjunction::from_pairs(preds, path);
                Ok(Constraint::Pred(pred?))
            },
        }
    }

    pub fn from_where_block_opt<'i>(
        where_block: &Option<pairs::WhereBlock<'i>>,
        path: &'i std::path::Path,
    ) -> Result<Vec<Self>, PredicateError<'i>> {
        if let Some(where_block) = where_block
            && let Some(constraints) = where_block.ConstraintsSeparatedByComma()
        {
            let (first, following, _) = constraints.get_matched();
            let following = following
                .iter_matched()
                .map(|comma_with_elem| comma_with_elem.get_matched().1);
            // FIXME: this is will return early errors if any of the constraints are invalid, consider
            // collecting all errors instead
            std::iter::once(first)
                .chain(following)
                .map(|pair| Self::from_pairs(pair, path))
                .collect()
        } else {
            Ok(Vec::new())
        }
    }
}
