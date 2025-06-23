# Usage

## The Pattern Language

### Notes

- When the operand has a `Copy` type, operator `Copy` or `Move` are considered equivalent.
- When writing a fn pattern item, `pub fn` only matches public functions, `pub(restricted) fn` only matches non-public functions, and `fn` matches all functions.
- When writing a fn pattern item, `unsafe fn` only matches unsafe functions, `fn` only matches safe functions, and `unsafe? fn` matches all functions.
- Use `#[deduplicate]` on pattern items to avoid duplication (it costs, so take your care).
- Use `#[output = "foo"]` on fn pattern items to bind its output span with `foo`.
- `fn $foo` binds span of the function to `foo`.
- When writing a fn pattern item, `#[inline] fn` only matches functions annotated with `#[inline]` or `#[inline(always)]`, `#[inline(always)] fn` only matches functions annotated with `#[inline(always)]`, `#[inline(never)] fn` only matches functions annotated with `#[inline(never)]`, `#[inline(any)] fn` only matches functions not annotated with `#[inline(never)]`, and `fn` matches all functions.
