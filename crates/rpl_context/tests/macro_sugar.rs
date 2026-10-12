#![feature(rustc_private)]
#![feature(let_chains)]
#![recursion_limit = "256"]

extern crate rustc_arena;
extern crate rustc_span;

use std::path::Path;
use std::sync::LazyLock;

use pest_typed::ParsableTypedNode;
use rpl_constraints::predicates::PredicateKind;
use rpl_context::{PatternCtxt, pat};
use rpl_meta::arena::Arena;
use rpl_meta::context::MetaContext;
use rpl_meta::symbol_table::{SymbolTable, WithPath};
use rpl_parser::pairs;
use rustc_span::Symbol;

fn with_pattern_item(source: &str, f: impl FnOnce(&mut pat::PatternItem<'_>)) {
    rustc_span::create_session_if_not_set_then(rustc_span::edition::LATEST_STABLE_EDITION, |_| {
        static ARENA: LazyLock<Arena> = LazyLock::new(Arena::default);
        static MCTX: LazyLock<MetaContext> = LazyLock::new(|| MetaContext::new(&ARENA));

        let source = source.replace("$ ", "$");
        let source = ARENA.alloc_str(&source);
        let pat_item = pairs::RPLPatternItem::try_parse(source).unwrap_or_else(|err| panic!("{err}"));
        let pat_item: &'static _ = Box::leak(Box::new(pat_item));

        let mut errors = Vec::new();
        let path = Path::new(file!());
        MCTX.set_active_path(Some(path));
        let symbol_tables = SymbolTable::collect_symbol_tables(&MCTX, &[], std::iter::once(pat_item), &mut errors);
        assert!(errors.is_empty(), "symbol table errors: {errors:?}");
        let symbol_tables: &'static _ = Box::leak(Box::new(symbol_tables));

        PatternCtxt::entered_no_tcx(move |pcx| {
            let pat = pcx.new_pattern();
            let ident = Symbol::intern(pat_item.Identifier().span.as_str());
            pat.add_pattern_item(WithPath::new(path, pat_item), &symbol_tables, pat::PattOrUtil::Patt);
            let item = pat.patt_block.get_mut(&ident).unwrap();
            f(item);
        });
    });
}

#[test]
fn without_macro_desugars_to_difference() {
    with_pattern_item(
        "p[$T: type] = fn _ (..) -> _ { let $x: $T = _; without!(_ = $f(copy $x)); }",
        |item| match item {
            pat::PatternItem::RPLPatternOperation(op) => {
                assert_eq!(op.positive.len(), 1, "one positive body");
                assert_eq!(op.negative.len(), 1, "one negative body");
            },
            _ => panic!("expected a pattern operation"),
        },
    );
}

#[test]
fn or_macro_desugars_to_union() {
    with_pattern_item(
        "p[$T: type] = fn _ (..) -> _ { let $x: $T = _; or!(_ = $f(copy $x), _ = $g(copy $x)); }",
        |item| match item {
            pat::PatternItem::RPLPatternOperation(op) => {
                assert_eq!(op.positive.len(), 2, "two alternatives");
                assert_eq!(op.negative.len(), 0, "no negative body");
            },
            _ => panic!("expected a pattern operation"),
        },
    );
}

#[test]
fn panic_macro_adds_may_panic_predicate() {
    with_pattern_item(
        "p[$T: type] = fn _ (..) -> _ { let $x: $T = _; 'sink: panic!(_ = $f(copy $x)); }",
        |item| match item {
            pat::PatternItem::RustItems(items) => {
                let fn_pat = items.fns.all_fns[0];
                let has_may_panic = fn_pat.constraints.preds.iter().any(|conj| {
                    conj.clauses.iter().any(|clause| {
                        clause
                            .terms
                            .iter()
                            .any(|term| matches!(term.kind, PredicateKind::MayPanic))
                    })
                });
                assert!(has_may_panic, "expected a `may_panic` predicate in the fn constraints");
            },
            _ => panic!("expected rust items"),
        },
    );
}
