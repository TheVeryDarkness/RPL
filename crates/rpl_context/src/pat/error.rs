use std::borrow::Cow;
use std::sync::LazyLock;

use rpl_parser::pairs;
use rustc_errors::LintDiagnostic;
use rustc_lint::{Level, Lint};
use rustc_middle::mir::Body;
use rustc_span::{Span, Symbol};
use sync_arena::declare_arena;

use super::{LabelMap, Matched};

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
    lint: &'static Lint,
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
    /// Also see [`rustc_session::declare_tool_lint!`].
    pub const fn lint(&self) -> &'static Lint {
        self.lint
    }
}

pub(crate) struct DynamicErrorBuilder<'i> {
    primary: (&'i str, &'i str),
    #[allow(dead_code)] //FIXME: handle this
    args: Vec<(&'i str, &'i str)>,
    labels: Vec<(&'i str, &'i str)>,
    notes: Vec<(&'i str, Option<&'i str>)>,
    helps: Vec<(&'i str, Option<&'i str>)>,
    lint: &'static Lint,
}

declare_arena!(
    [
        [] _phantom: &'tcx (),
    ]
);

static ARENA: LazyLock<Arena<'static>> = LazyLock::new(Arena::default);

impl<'i> DynamicErrorBuilder<'i> {
    pub(super) fn from_item(item: &'i pairs::diagBlockItem<'i>) -> (Self, Symbol) {
        let (ident, _, _, pairs, _, _) = item.get_matched();
        let pattern_name = Symbol::intern(ident.span.as_str());
        let mut primary = (None, None);
        let mut labels = Vec::new();
        let mut notes = Vec::new();
        let mut helps = Vec::new();
        let args = Vec::new();
        let mut level = Level::Deny;
        let mut name = None;

        for pair in pairs.iter_matched() {
            let (key, span, _, message, _) = pair.get_matched();

            let message = message.span.as_str();
            //FIXME: strip quotes from message
            let message = message.strip_prefix("\"").unwrap_or(message);
            let message = message.strip_suffix("\"").unwrap_or(message);

            let key = key.span.as_str();

            let span_name = span.as_ref().map(|span| span.get_matched().1.span.as_str());

            match key {
                "primary" => {
                    primary.0 = Some(message);
                    if let Some(span_name) = span_name {
                        primary.1 = Some(span_name);
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
                "name" => {
                    name = Some(message);
                },
                "level" => {
                    level = match message {
                        "allow" => Level::Allow,
                        "warning" => Level::Warn,
                        "deny" => Level::Deny,
                        "forbid" => Level::Forbid,
                        _ => unimplemented!("Unrecognized level: {message}"),
                    };
                },
                _ => unimplemented!("Unrecognized key: {key:?}"),
            }
        }
        let builder = DynamicErrorBuilder {
            primary: (primary.0.unwrap(), primary.1.unwrap()),
            args,
            labels,
            notes,
            helps,
            lint: ARENA.alloc(Lint {
                name: ARENA.alloc_str(&format!("rpl::{}", name.unwrap())),
                default_level: level,
                ..Lint::default_fields_for_macro()
            }),
        };
        (builder, pattern_name)
    }
    pub(crate) fn build<'tcx>(
        &self,
        label_map: &LabelMap,
        body: &Body<'tcx>,
        matched: &impl Matched<'tcx>,
    ) -> DynamicError {
        let primary = (
            Cow::Owned(self.primary.0.to_string()),
            matched.span(label_map, body, self.primary.1),
        ); // FIXME: use actual span
        let labels = self
            .labels
            .iter()
            .map(|(label, span)| (Cow::Owned(label.to_string()), matched.span(label_map, body, span)))
            .collect();
        let notes = self
            .notes
            .iter()
            .map(|(note, span)| {
                (
                    Cow::Owned(note.to_string()),
                    span.map(|span| matched.span(label_map, body, span)),
                )
            })
            .collect();
        let helps = self
            .helps
            .iter()
            .map(|(help, span)| {
                (
                    Cow::Owned(help.to_string()),
                    span.map(|span| matched.span(label_map, body, span)),
                )
            })
            .collect();
        let lint = self.lint;
        DynamicError {
            primary,
            labels,
            notes,
            helps,
            lint,
        }
    }
}
