use std::ops::Deref;

use rpl_meta::collect_elems_separated_by_comma;
use rpl_meta::symbol_table::WithPath;
use rpl_parser::generics::{Choice2, Choice3, Choice15};
use rpl_parser::pairs::{self};
use rustc_middle::mir;

use super::{FnSymbolTable, with_path};
use crate::PatCtxt;
use crate::pat::mir::Operand;

pub(crate) fn mutability_from_pair_mutability(pair: &pairs::Mutability<'_>) -> mir::Mutability {
    if pair.kw_mut().is_some() {
        mir::Mutability::Mut
    } else {
        mir::Mutability::Not
    }
}

pub(crate) fn mutability_from_pair_ptr_mutability(pair: &pairs::PtrMutability<'_>) -> mir::Mutability {
    if pair.kw_mut().is_some() {
        mir::Mutability::Mut
    } else {
        mir::Mutability::Not
    }
}

pub(crate) fn borrow_kind_from_pair_mutability(pair: &pairs::Mutability<'_>) -> mir::BorrowKind {
    if pair.kw_mut().is_some() {
        mir::BorrowKind::Mut {
            kind: mir::MutBorrowKind::Default,
        }
    } else {
        mir::BorrowKind::Shared
    }
}

pub(crate) fn binop_from_pair(pair: &pairs::MirBinOp<'_>) -> mir::BinOp {
    match pair.deref() {
        Choice15::_0(_kw_add) => mir::BinOp::Add,
        Choice15::_1(_kw_sub) => mir::BinOp::Sub,
        Choice15::_2(_kw_mul) => mir::BinOp::Mul,
        Choice15::_3(_kw_div) => mir::BinOp::Div,
        Choice15::_4(_kw_rem) => mir::BinOp::Rem,
        Choice15::_5(_kw_lt) => mir::BinOp::Lt,
        Choice15::_6(_kw_le) => mir::BinOp::Le,
        Choice15::_7(_kw_gt) => mir::BinOp::Gt,
        Choice15::_8(_kw_ge) => mir::BinOp::Ge,
        Choice15::_9(_kw_eq) => mir::BinOp::Eq,
        Choice15::_10(_kw_ne) => mir::BinOp::Ne,
        Choice15::_11(_kw_bit_and) => mir::BinOp::BitAnd,
        Choice15::_12(_kw_bit_or) => mir::BinOp::BitOr,
        Choice15::_13(_kw_bit_xor) => mir::BinOp::BitXor,
        Choice15::_14(_kw_offset) => mir::BinOp::Offset,
    }
}

pub(crate) fn nullop_from_pair<'pcx>(pair: &pairs::MirNullOp<'_>) -> mir::NullOp<'pcx> {
    match pair.deref() {
        Choice2::_0(_kw_size_of) => mir::NullOp::SizeOf,
        Choice2::_1(_kw_align_of) => mir::NullOp::AlignOf,
    }
}

pub(crate) fn unop_from_pair(pair: &pairs::MirUnOp<'_>) -> mir::UnOp {
    match pair.deref() {
        Choice3::_0(_kw_neg) => mir::UnOp::Neg,
        Choice3::_1(_kw_not) => mir::UnOp::Not,
        Choice3::_2(_kw_ptr_metadata) => mir::UnOp::PtrMetadata,
    }
}

pub(crate) fn collect_operands<'pcx>(
    operands: Option<WithPath<'pcx, &'pcx pairs::MirOperandsSeparatedByComma<'pcx>>>,
    pcx: PatCtxt<'pcx>,
    fn_sym_tab: &'pcx FnSymbolTable<'pcx>,
) -> Vec<Operand<'pcx>> {
    if let Some(operands) = operands {
        let p = operands.path;
        collect_elems_separated_by_comma!(operands)
            .map(|operand| Operand::from(with_path(p, operand), pcx, fn_sym_tab))
            .collect()
    } else {
        vec![]
    }
}
