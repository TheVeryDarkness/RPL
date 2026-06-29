# Usage

## The Pattern Language

### Notes

- When the operand has a `Copy` type, operator `Copy` or `Move` are considered equivalent.
- Use `#[deduplicate]` on pattern items to avoid duplication (it costs, so take your care).
- For fn items in patterns, `pub fn` only matches public functions, `pub(restricted) fn` only matches non-public functions, and `fn` matches all functions.
- For fn items in patterns, `unsafe fn` only matches unsafe functions, `fn` only matches safe functions, and `unsafe? fn` matches all functions.
- For fn items in patterns,  `#[inline] fn` only matches functions annotated with `#[inline]` or `#[inline(always)]`, `#[inline(always)] fn` only matches functions annotated with `#[inline(always)]`, `#[inline(never)] fn` only matches functions annotated with `#[inline(never)]`, `#[inline(any)] fn` only matches functions not annotated with `#[inline(never)]`, and `fn` matches all functions.
- Use `#[output = "foo"]` on fn items in patterns to bind its output span with `foo`.
- `fn $foo` binds `$foo` with the span of the function.

### Multi-function and type matching (MatchSession)

- A single `RustItems` block may contain multiple `fn` patterns, `struct`/`enum` patterns, or both. The driver builds a crate index and runs **MatchSession** with CSP backtracking so that shared type metavariables (for example `$T` in both a struct field type and a function body) must bind consistently across all assigned slots.
- `fn _` patterns match at most one Rust function per session result. Named function patterns (including `$name` metas) participate in full M:N assignment: any Rust function may fill a slot as long as global metavar bindings merge without conflict.
- Struct and enum patterns in the same block are matched against all ADT items in the crate and merged into the same binding environment before function slots are solved.
- Signature-only function patterns (empty MIR body in the `.rpl` file) skip MIR subgraph matching and evaluate item-level constraints only.
- Session result count is capped by `DEFAULT_MAX_SESSION_RESULTS` (256) in `rpl_match::session` to avoid pathological M:N explosion; increase via `SessionConfig::max_results` when wiring custom drivers.