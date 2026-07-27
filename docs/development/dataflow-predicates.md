# Dataflow predicates (`flows_to` / `may_panic`)

RPL can express **source → sink** dataflow constraints in `where` blocks, on top of MIR CFG/DDG subgraph matching.

## Syntax

```rpl
p[...] = unsafe? fn _(..) -> _ {
    'src:
    // statement that defines / affects `$x`
    ...
    'sink:
    // statement that uses `$x` (or a Call site)
} where {
    flows_to($x, 'src, 'sink),
    may_panic('sink),
}
```

| Predicate | Arguments | Meaning |
|-----------|-----------|---------|
| `flows_to($x, 'src, 'sink)` | local/place + two labels | DDG path labeled with `$x` from `'src` to `'sink` (intermediate statements allowed). |
| `may_panic('sink)` | one label | Potential panic site: MIR `Assert`; trait methods whose call-site args still contain type/const params (Rudra unresolvable-generic); local generic fns; `Fn*` / closure / `dyn`. Concrete trait calls (e.g. std `Zip`/`Chunks` `next`) and known non-panicking helpers (`mem::forget`, `ptr::read`, …) are excluded. |

`where` attaches to a single `fn` item, not the outer multi-item `{ ... }` brace.

## vs subgraph matching

- **`match_ddg`**: requires **direct** DDG edges between matched pattern statements.
- **`flows_to`**: only **reachability** of `$x` between two anchors.

## Engine

- [`DataDepGraph::flows_to`](../../crates/rpl_mir_graph/src/graph.rs)
- [`PredicateEvaluator`](../../crates/rpl_match/src/predicate_evaluator.rs) (MIR DDG injected from MatchSession)

Prefer `-Z inline-mir=false` in UI tests when matching `Vec::set_len` / `ptr::*` as Calls (otherwise RPL may inline them to intrinsics/field writes).

## Example matrix

| Sample | Role | Location |
|--------|------|----------|
| Feature `flows_to` | Lock `flows_to` TP/TN | `tests/ui/features/flows_to/` |
| CVE-2020-25795 | `copy_nonoverlapping` then panicking callback | `docs/patterns-pest/cve/CVE-2020-25795.rpl` |
| CVE-2020-35923 | `NotNan` field write + `is_nan` / `unreachable_unchecked` | `docs/patterns-pest/cve/CVE-2020-35923.rpl` |
| Retain-like synth | `set_len(0)` then callback | `tests/ui/features/panic_safety_retain/` |

## Pattern tips (Panic Safety)

- **Qself**: `<I as Iterator>::next` / `<T as Float>::is_nan` are not matched by path forms. Declare `fn $cb(...);` / `fn $isnan(...);` and call `_ = $cb(_);`.
- **Ignore-ret calls**: prefer `_ = $cb(_);` over `$x = $cb(_);` when binding the return is unnecessary.
- **Decls before stmts**: all `let` locals must appear before non-decl MIR statements in a pattern block.
- **`copy_nonoverlapping`**: use the MIR statement form (`copy_nonoverlapping(_, _, _);`); the ident is a keyword and cannot appear in a Path.

## Limitations

- Root-local only for `flows_to`; no unwind-edge requirement.
- `$callback` / `$cb` Call matching is by signature — pair with `may_panic` and distinctive signatures.
- Function *names* in patterns do not filter which MIR items are searched (`fn retain_like_tp` still tries other bodies).
