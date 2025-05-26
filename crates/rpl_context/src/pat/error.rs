use std::borrow::Cow;

use rpl_parser::pairs;
use rustc_errors::LintDiagnostic;
use rustc_lint::Lint;
use rustc_span::{Span, Symbol};

use super::Matched;

/// A dynamic error that can be used to report user-defined errors
///
/// See:
/// - [`rustc_errors::LintDiagnostic`].
/// - [`rustc_macros::diagnostics::lint_diagnostic_derive`].
/// - [`rustc_macros::LintDiagnostic`].
pub struct DynamicError {
    /// Primary message and its span.
    ///
    /// See [`rustc_errors::Diag::primary_message`].
    /// The primary message is the main error message that will be displayed to the user.
    primary: (Cow<'static, str>, Span),
    /// Label description, and the span of the label.
    ///
    /// See [`rustc_errors::Diag::span_label`].
    /// Labels are used to highlight specific parts of the code that are relevant to the error.
    labels: Vec<(Cow<'static, str>, Span)>,
    notes: Vec<(Cow<'static, str>, Option<Span>)>,
    helps: Vec<(Cow<'static, str>, Option<Span>)>,
}

impl LintDiagnostic<'_, ()> for DynamicError {
    fn decorate_lint(self, diag: &mut rustc_errors::Diag<'_, ()>) {
        let primary_message = self.primary.0;
        diag.primary_message(primary_message);
        for (label, span) in self.labels {
            diag.span_label(span, label);
        }
        for (help, span_help) in self.helps {
            if let Some(span_help) = span_help {
                diag.span_help(span_help, help);
            } else {
                diag.help(help);
            }
        }
        for (note, span_note) in self.notes {
            if let Some(span_note) = span_note {
                diag.span_note(span_note, note);
            } else {
                diag.note(note);
            }
        }
    }
}

impl DynamicError {
    pub const fn primary_span(&self) -> Span {
        self.primary.1
    }
    pub(crate) const fn lint(&self) -> &'static Lint {
        const LINT: Lint = Lint {
            name: "RPL::DYNAMIC",
            desc: "dynamic RPL pattern",
            ..Lint::default_fields_for_macro()
        };
        &LINT
    }
    // const fn attr_error(span: Span) -> DynamicError {
    //     DynamicError {
    //         primary: (Cow::Borrowed("Ill-formed RPL dynamic attribute"), span),
    //         labels: Vec::new(),
    //         notes: Vec::new(),
    //         helps: Vec::new(),
    //     }
    // }
    fn unknown_attribute_error(span: Span) -> Self {
        Self {
            primary: (Cow::Borrowed("Unknown attribute key"), span),
            labels: Vec::new(),
            notes: vec![(
                Cow::Borrowed("Allowed attribute keys are: `primary_message`, `labels`, `note`, `help`"),
                None,
            )],
            helps: Vec::new(),
        }
    }
    const fn missing_primary_message_error(attr: &rustc_hir::Attribute) -> Self {
        Self {
            primary: (Cow::Borrowed("Missing primary message"), attr.span),
            labels: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }
    fn item_to_value_str(item: &rustc_ast::MetaItemInner) -> Result<Symbol, Self> {
        item.value_str().ok_or_else(|| {
            // If the value is not a string, we return an error.
            // This is a fallback to ensure that we always return a valid error.
            Self {
                primary: (Cow::Borrowed("Expected a string value"), item.span()),
                labels: Vec::new(),
                notes: Vec::new(),
                helps: Vec::new(),
            }
        })
    }
    const fn expected_meta_item_list_error(span: Span) -> Self {
        Self {
            primary: (Cow::Borrowed("Expected a meta item list"), span),
            labels: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }
    fn attr_to_meta_item_list(
        attr: &rustc_hir::Attribute,
    ) -> Result<impl Iterator<Item = rustc_ast::MetaItemInner>, Self> {
        attr.meta_item_list().map_or_else(
            || Err(Self::expected_meta_item_list_error(attr.span())),
            |vec| Ok(vec.into_iter()),
        )
    }
    fn item_to_meta_item_list(
        item: &rustc_ast::MetaItemInner,
    ) -> Result<impl Iterator<Item = &rustc_ast::MetaItemInner>, Self> {
        item.meta_item_list().map_or_else(
            || Err(Self::expected_meta_item_list_error(item.span())),
            |vec| Ok(vec.iter()),
        )
    }
    fn from_attr_impl(attr: &rustc_hir::Attribute, span: Span) -> Result<DynamicError, DynamicError> {
        let items = Self::attr_to_meta_item_list(attr)?;
        let mut primary_message = None;
        let mut labels = Vec::new();
        let mut notes = Vec::new();
        let mut helps = Vec::new();
        for item in items {
            match item.name_or_empty().as_str() {
                "primary_message" => {
                    primary_message = Some(Cow::Owned(Self::item_to_value_str(&item)?.to_string()));
                },
                "labels" => {
                    let label_list = Self::item_to_meta_item_list(&item)?;
                    for label_item in label_list {
                        // FIXME: `label_item.span()` is not the actual span it refers to,
                        labels.push((
                            Cow::Owned(Self::item_to_value_str(label_item)?.to_string()),
                            label_item.span(),
                        ));
                    }
                },
                "note" => {
                    notes.push((Cow::Owned(Self::item_to_value_str(&item)?.to_string()), None));
                },
                "help" => {
                    helps.push((Cow::Owned(Self::item_to_value_str(&item)?.to_string()), None));
                },
                _ => {
                    // error!("Unknown attribute key {:?}", item.name_or_empty())
                    return Err(Self::unknown_attribute_error(item.span()));
                },
            }
        }
        let primary_message = primary_message.ok_or_else(|| Self::missing_primary_message_error(attr))?;
        let primary = (primary_message, span);
        Ok(DynamicError {
            primary,
            labels,
            notes,
            helps,
        })
    }
    pub(crate) fn from_attr(attr: &rustc_hir::Attribute, span: Span) -> DynamicError {
        Self::from_attr_impl(attr, span).unwrap_or_else(|err| {
            // If we fail to parse the attribute, we return an error.
            // This is a fallback to ensure that we always return a valid error.
            err
        })
    }
}

pub(crate) struct DynamicErrorBuilder<'i> {
    primary: (&'i str, &'i str),
    args: Vec<(&'i str, &'i str)>,
    labels: Vec<(&'i str, &'i str)>,
    notes: Vec<(&'i str, Option<&'i str>)>,
    helps: Vec<(&'i str, Option<&'i str>)>,
}

impl<'i> DynamicErrorBuilder<'i> {
    pub(super) fn from_item(item: &'i pairs::diagBlockItem<'i>) -> (Self, Symbol) {
        let (ident, _, _, pairs, _, _) = item.get_matched();
        let name = Symbol::intern(ident.span.as_str());
        let mut primary = (None, None);
        let mut labels = Vec::new();
        let mut notes = Vec::new();
        let mut helps = Vec::new();
        let args = Vec::new();

        for pair in pairs.iter_matched() {
            let (key, span, _, message, _) = pair.get_matched();
            let message = message.span.as_str();
            let span_name = span.as_ref().map(|s| s.get_matched().1.span.as_str());
            match key.span.as_str() {
                "primary_message" => {
                    primary.0 = Some(message);
                    if let Some(span) = span_name {
                        primary.1 = Some(span);
                    }
                },
                "args" => {
                    labels.push((message, span_name.unwrap()));
                },
                "label" => {
                    labels.push((message, span_name.unwrap()));
                },
                "note" => {
                    notes.push((message, span_name));
                },
                "help" => {
                    helps.push((message, span_name));
                },
                _ => unimplemented!(),
            }
        }
        let builder = DynamicErrorBuilder {
            primary: (primary.0.unwrap_or("Unknown primary message"), primary.1.unwrap_or("")),
            args,
            labels,
            notes,
            helps,
        };
        (builder, name)
    }
    pub(crate) fn build<'tcx>(&self, matched: &impl Matched<'tcx>) -> DynamicError {
        let primary = (
            Cow::Owned(self.primary.0.to_string()),
            matched.named_span(self.primary.1),
        ); // FIXME: use actual span
        let labels = self
            .labels
            .iter()
            .map(|(label, span)| (Cow::Owned(label.to_string()), matched.named_span(span)))
            .collect();
        let notes = self
            .notes
            .iter()
            .map(|(note, span)| (Cow::Owned(note.to_string()), span.map(|span| matched.named_span(span))))
            .collect();
        let helps = self
            .helps
            .iter()
            .map(|(help, span)| (Cow::Owned(help.to_string()), span.map(|span| matched.named_span(span))))
            .collect();
        DynamicError {
            primary,
            labels,
            notes,
            helps,
        }
    }
}
