---
name: lean-audit
description: "Invoke whenever writing, changing, reviewing, or sweeping Ostendo code, tests, comments, or docs. Authoring gate that blocks unnecessary code, tests, and comments, plus an audit workflow for removing low-value tests, dead code, test-only seams, and noise comments."
---

# Lean Audit

One value bar for everything that lands in this repo: code, tests, comments,
and docs must each pay for their maintenance cost. Three modes:

- **Authoring gate**: every new or changed line of code, test, or comment.
- **Audit**: a focused sweep of one module for junk (below); lands as one
  coherent commit.
- **Campaign**: prune one whole subsystem (`render/engine/`, `markdown/`,
  `image_util/`, …). Inventory every file it owns first, then land it as a
  series of audit-sized commits, one owner boundary at a time.

Optimize for confidence, not deletion count.

## Authoring gate: code

Before adding code, answer; a missing answer means do not write it yet:

1. Which user-visible behavior or existing caller needs it today? "Future
   use", `#[allow(dead_code)]`, and "for completeness" enum variants are not
   callers.
2. Does something already do it? Search `src/` for an existing helper
   (`render/text.rs`, `theme/colors.rs`, `render/engine/output.rs`,
   `markdown/regex_patterns.rs`) before writing a new one.
3. Is this the smallest change at the owning module? No speculative config,
   traits with one impl, builder types for three fields, or wrappers that only
   forward.
4. Does it need visibility (`pub`, `pub(crate)`) or a parameter that only a
   test needs? If yes, test through the real entry point instead.

## Authoring gate: comments and docs

- A comment explains *why* (a terminal quirk, a protocol constraint, an
  ordering requirement), never *what* the next line obviously does.
- Doc comments go on items whose contract is not obvious from the name and
  signature. Do not add `/// Returns the width.` to `fn width()`.
- No history in code: no "fixed in v0.4.2", "previously", "new:", "moved
  from", or commented-out code. Git has the history.
- No section-banner comments, no restated parameter lists, no `# Returns`
  blocks that repeat the return type.
- Docs never hard-code counts that rot (line counts, test counts, file sizes).
  One source of truth per fact; link instead of copying.

## Authoring gate: tests

Before adding any test, answer four questions; a missing answer means do not
add it yet:

1. What observable behavior, invariant, or independent contract does it
   protect?
2. What credible regression makes it fail?
3. Why does existing coverage not already catch that failure? Each contract
   has one primary test owner at the strongest boundary (usually
   `parse_presentation()` for syntax, the public render/export function for
   output). Prefer adding a case to an existing table-driven test over a
   near-duplicate test.
4. Does it need a production seam (`pub(crate)` export, flag, wrapper,
   injection hook) that no production caller needs? If yes, move the test to
   the real boundary instead.

Then check it against every [junk pattern](#junk-patterns); a match fails the
gate unless the [retention bar](#retention-bar) names the contract it guards.
A test that breaks under behavior-preserving refactoring asserts
implementation, not behavior.

Bug regression tests must fail on the pre-fix code for the intended reason and
pass after the fix. One regression at the owner boundary covers the bug; do
not replay it at every layer.

## Junk patterns

### Tests

- assertion-free coverage probes (`let _ = f(x);`, `assert!(r.is_ok())` where
  the value is the point);
- self-comparisons, identity round-trips, and asserting a `Default`/`new()`
  returns the literal it is built from;
- copied fixtures, inventories, or lists (theme counts, enum variant lists);
- exact source greps or string-shape checks of internal formatting;
- private predicate or call-shape tests duplicated by a real boundary test;
- duplicate invocations of the same contract with trivially different input;
- expected values produced by the helper under test;
- tests whose only purpose is keeping a test-only `pub(crate)` item alive;
- env-var tests that mutate process state without the shared lock, or that
  pass because of the host terminal (`KITTY_WINDOW_ID`, `TERM_PROGRAM`);
- negative controls that pass for an unrelated reason;
- names that promise more than the input exercises.

### Code

- dead code kept alive by `#[allow(dead_code)]` or only called from tests;
- enum variants, struct fields, or parameters nothing reads;
- duplicated helpers (hex parsing, width math, wrapping, ANSI parsing) that
  already exist elsewhere;
- `.clone()` of slides, buffers, or images in the per-frame path to dodge the
  borrow checker when a borrow or `Arc` would do;
- recompiling regexes, re-reading files, or re-highlighting code every frame
  instead of caching;
- `.unwrap()` in non-test code on input that comes from the user's markdown,
  the terminal, the network, or the filesystem.

### Comments

- comments restating the code, banners, stale counts, version history,
  commented-out code, and TODOs without an owner or issue.

## Retention bar

Keep a test when it independently enforces a user-facing contract:

- markdown/directive syntax accepted by `parse_presentation()`;
- theme YAML schema and WCAG contrast thresholds;
- security limits (exec timeout, input/output caps, `--no-exec`,
  `--remote-exec`, token auth, connection cap);
- persisted state file format (`presentation/state.rs`);
- terminal protocol bytes (Kitty, iTerm2, Sixel, OSC 66) and ANSI output;
- HTML/PDF export output and CLI flags;
- call ordering when order is observable (transition → entrance → loop);
- regressions with a credible failure mode.

Static or fast is not a deletion reason. A test that resembles implementation
may still be the only independent guard; prove otherwise before removing it.
A retained test that fails on the baseline is a possible product bug:
reproduce it and repair the owner rather than deleting it.

## Candidate evidence

Record before deleting a test or production seam; a missing field means the
candidate is not ready:

- exact name and `file:line`;
- what failure it can actually detect;
- non-test callers of the covered code (`rg` for them);
- stronger remaining proof at the owner boundary, or why none is needed;
- production or test-support deletion it unlocks;
- risk and the focused validation command.

## Edit shape

One coherent owner-boundary batch per commit. Delete obsolete test-only
exports, wrappers, and dead production paths outright; do not leave aliases.
Move retained regressions to their canonical owner. Prefer net-negative
production LOC. Do not add replacement tests that restate the same
implementation, and do not convert uncertain candidates into cleanup to raise
deletion counts.

## Validation

1. `cargo test <module_or_test_filter>` for the owner and its siblings.
2. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.
3. `cargo test` (full suite).
4. `cargo run --release -- --validate presentations/examples/test_presentation.md`
   whenever parsing, rendering, or directives change.
5. `git diff --check`, then `git diff --numstat`; report production and test
   LOC separately.

## Handoff

Report: removed categories and why, production simplifications, retained
false positives and why they stay, validation actually run, production vs
test LOC, and named follow-ups.
