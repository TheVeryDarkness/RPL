use std::fmt;

use rustc_index::IndexVec;
use rustc_middle::mir::{self};
use rustc_middle::ty::{self, TyCtxt};

pub struct BodyInfoCache {
    /// `product_of[i][j]` is `Some(true)` if `i` may be a product of `j`, `Some(false)` if `i` may
    /// be a quotient of `j`, and `None` if there is no relationship.
    product_of: IndexVec<mir::Local, IndexVec<mir::Local, Option<bool>>>,
    // /// `derive_from[i][j]` is `true` if `i` may be computed from `j`, `false` if there is no
    // /// relationship.
    // derive_from: IndexVec<mir::Local, IndexVec<mir::Local, bool>>,
}

impl fmt::Debug for BodyInfoCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(
                self.product_of
                    .iter_enumerated()
                    .flat_map(|(i, j)| j.iter_enumerated().filter_map(move |(k, v)| v.map(|b| (i, k, b)))),
            )
            .finish()
    }
}

impl BodyInfoCache {
    #[instrument(level = "debug", skip(body), ret)]
    pub fn new(body: &mir::Body<'_>) -> Self {
        let n = body.local_decls.len();
        // Track the product relationship among locals, true for product, false for quotient
        let mut product_of: IndexVec<mir::Local, IndexVec<mir::Local, Option<bool>>> =
            IndexVec::from_fn_n(|_| IndexVec::from_elem_n(None, n), n);
        for i in 0..n {
            let i = mir::Local::from_usize(i);
            product_of[i][i] = Some(true);
        }
        for local in body.basic_blocks.iter() {
            for stmt in &local.statements {
                // Check if the statement is a product or quotient
                if let mir::StatementKind::Assign(box (ref lhs, ref rhs)) = stmt.kind
                    && let Some(lhs) = lhs.as_local()
                {
                    match rhs {
                        mir::Rvalue::BinaryOp(
                            mir::BinOp::Mul
                            | mir::BinOp::MulUnchecked
                            | mir::BinOp::MulWithOverflow
                            | mir::BinOp::Add
                            | mir::BinOp::AddUnchecked
                            | mir::BinOp::AddWithOverflow
                            | mir::BinOp::Sub
                            | mir::BinOp::SubUnchecked
                            | mir::BinOp::SubWithOverflow,
                            box rhs,
                        ) => {
                            if let mir::Operand::Copy(rhs1) | mir::Operand::Move(rhs1) = rhs.0
                                && let Some(rhs1) = rhs1.as_local()
                            {
                                product_of[lhs][rhs1] = Some(true);
                            }
                            if let mir::Operand::Copy(rhs2) | mir::Operand::Move(rhs2) = rhs.1
                                && let Some(rhs2) = rhs2.as_local()
                            {
                                product_of[lhs][rhs2] = Some(true);
                            }
                        },
                        mir::Rvalue::BinaryOp(mir::BinOp::Div, box rhs) => {
                            if let mir::Operand::Copy(rhs1) | mir::Operand::Move(rhs1) = rhs.0
                                && let Some(rhs1) = rhs1.as_local()
                            {
                                product_of[lhs][rhs1] = Some(true);
                            }
                            if let mir::Operand::Copy(rhs2) | mir::Operand::Move(rhs2) = rhs.1
                                && let Some(rhs2) = rhs2.as_local()
                            {
                                product_of[lhs][rhs2] = Some(false);
                            }
                        },
                        mir::Rvalue::Use(mir::Operand::Copy(rhs) | mir::Operand::Move(rhs))
                        | mir::Rvalue::Cast(_, mir::Operand::Copy(rhs) | mir::Operand::Move(rhs), _) => {
                            if let Some(rhs) = rhs.as_local() {
                                product_of[lhs][rhs] = Some(true);
                            }
                        },
                        _ => (),
                    }
                }
            }
        }
        for j in 0..n {
            let j = mir::Local::from_usize(j);
            for i in 0..n {
                for k in 0..n {
                    if i == k {
                        continue;
                    }
                    let i = mir::Local::from_usize(i);
                    let k = mir::Local::from_usize(k);
                    if let (Some(s1), Some(s2)) = (product_of[i][j], product_of[j][k]) {
                        product_of[i][k] = Some(s1 == s2);
                    }
                }
            }
        }
        Self { product_of }
    }
}

// FIX: consider a more general way for error handling
pub type MultipleLocalsPredsFnPtr =
    for<'tcx> fn(TyCtxt<'tcx>, ty::TypingEnv<'tcx>, &mir::Body<'tcx>, &BodyInfoCache, Vec<mir::Local>) -> bool;

/// Check if former local is a product of latter local for every two consecutive locals
#[instrument(level = "debug", skip(cache), ret)]
pub(crate) fn product_of<'tcx>(
    _: TyCtxt<'tcx>,
    _: ty::TypingEnv<'tcx>,
    _: &mir::Body<'tcx>,
    cache: &BodyInfoCache,
    locals: Vec<mir::Local>,
) -> bool {
    locals.windows(2).all(|pair| {
        let (first, second) = (pair[0], pair[1]);
        cache.product_of[first][second].unwrap_or(false)
    })
}
