use std::fmt::Debug;
use std::hash::Hash;

use either::Either;
use rpl_constraints::predicates;
use rpl_meta::check::Inline;
use rpl_meta::collect_elems_separated_by_comma;
use rpl_parser::generics::Choice2;
use rpl_parser::pairs;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_span::Symbol;

#[derive(Default)]
pub struct PatAttr<'i> {
    pub(super) deduplicate: bool,
    pub(super) diag: Option<Symbol>,
    pub(super) consts: FxHashMap<Symbol, &'i str>,
}

impl<'i> PatAttr<'i> {
    pub(in super::super) fn parse_all<'a>(attrs: impl Iterator<Item = &'a pairs::Attr<'i>>) -> Self
    where
        'i: 'a,
    {
        let mut result = Self::default();
        for attr in attrs {
            result.parse(attr);
        }
        result
    }
    fn parse(&mut self, pairs: &pairs::Attr<'i>) {
        let (_, _, attrs, _) = pairs.get_matched();
        for attr in collect_elems_separated_by_comma!(attrs) {
            let (key, value) = attr.get_matched();
            match key.span.as_str() {
                // #[deduplicate]
                "deduplicate" => match value {
                    Some(_) => unreachable!(),
                    None => {
                        self.deduplicate = true;
                    },
                },
                // #[diag = "diag_name"]
                "diag" => match value {
                    Some(Choice2::_0(_)) | None => unreachable!(),
                    Some(Choice2::_1(msg)) => {
                        let (_, msg) = msg.get_matched();
                        self.diag = Some(Symbol::intern(msg.diagMessageInner().span.as_str()));
                    },
                },
                // #[const(name1 = "...", name2 = "."]
                "const" => match value {
                    Some(Choice2::_1(_)) | None => unreachable!(),
                    Some(Choice2::_0(list)) => {
                        let (_, list, _) = list.get_matched();
                        if let Some(list) = list {
                            for pair in collect_elems_separated_by_comma!(list) {
                                let (name, value) = pair.get_matched();
                                let name = Symbol::intern(name.span.as_str());
                                let value = value
                                    .as_ref()
                                    .unwrap()
                                    ._1()
                                    .unwrap()
                                    .1
                                    .matched
                                    .diagMessageInner()
                                    .span
                                    .as_str();
                                self.consts.insert(name, value);
                            }
                        }
                    },
                },
                _ => unreachable!(),
            }
        }
    }

    pub fn post_process<M: Eq + Hash + Debug>(&self, iter: impl Iterator<Item = M>) -> impl Iterator<Item = M> {
        match self.deduplicate {
            true => Either::Left(iter.collect::<FxHashSet<_>>().into_iter()),
            false => Either::Right(iter),
        }
    }
}

#[derive(Default, Debug)]
pub struct FnAttr {
    pub(super) output: Option<Symbol>,
    pub(super) inline: Option<Inline>,
    pub(super) predicates: Vec<predicates::SingleFnPredsFnPtr>,
}

impl FnAttr {
    pub(super) fn parse<'i>(pairs: impl Iterator<Item = &'i pairs::Attr<'i>>) -> Self {
        let mut result = Self::default();
        for pairs in pairs {
            let (_, _, attrs, _) = pairs.get_matched();
            for attr in collect_elems_separated_by_comma!(attrs) {
                let (key, value) = attr.get_matched();
                match key.span.as_str() {
                    "output" => match value {
                        Some(Choice2::_0(_)) | None => unreachable!(),
                        Some(Choice2::_1(msg)) => {
                            let (_, msg) = msg.get_matched();
                            result.output = Some(Symbol::intern(msg.diagMessageInner().span.as_str()));
                        },
                    },
                    "inline" => match value {
                        Some(Choice2::_1(_)) | None => unreachable!(),
                        Some(Choice2::_0(msg)) => {
                            let (_, msg, _) = msg.get_matched();
                            if let Some(msg) = msg {
                                let inner = collect_elems_separated_by_comma!(msg).collect::<Vec<_>>();
                                if inner.len() != 1 {
                                    panic!("Expected exactly one output, found {}", inner.len());
                                }
                                let (level, attr) = inner[0].get_matched();
                                assert!(attr.is_none(), "Unexpected attribute in output: {attr:?}");
                                result.inline = match level.span.as_str() {
                                    "always" => Some(Inline::Always),
                                    "any" => Some(Inline::Any),
                                    "never" => Some(Inline::Never),
                                    _ => panic!("Unexpected inline level: {}", level.span.as_str()),
                                };
                            } else {
                                result.inline = Some(Inline::Normal);
                            }
                        },
                    },
                    "rpl" => match value {
                        Some(Choice2::_1(_)) | None => unreachable!(),
                        Some(Choice2::_0(inner)) => {
                            let (_, inner, _) = inner.get_matched();
                            if let Some(inner) = inner {
                                for inner in collect_elems_separated_by_comma!(inner) {
                                    let (key, value) = inner.get_matched();
                                    assert!(value.is_none(), "Unexpected value in RPL attribute: {value:?}");
                                    match key.span.as_str() {
                                        "requires_monomorphization" => {
                                            result.predicates.push(predicates::requires_monomorphization);
                                        },
                                        _ => panic!("Unexpected RPL attribute: {}", key.span.as_str()),
                                    }
                                }
                            }
                        },
                    },
                    _ => unreachable!(),
                }
            }
        }
        result
    }
}
