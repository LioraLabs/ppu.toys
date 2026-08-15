# vendor/piccolo — piccolo 0.3.3 with one upstream fix backported

This is not a fork we intend to maintain. It is crates.io `piccolo` 0.3.3, byte
for byte, with one already-upstream bug fix applied because upstream never
released it. **Delete this directory as soon as either condition below is met.**

## Why it exists

`RawTable::set` in piccolo 0.3.3 aborts the VM — a Rust panic, not a catchable
Lua error — on two shapes of ordinary Lua:

```lua
local t = { k0=1, k1=1, k2=1, 1,2,3,4,5,6 }; t.zz = 1  -- "map slot must be pre-reserved"
local t = {}; t.a=1; t.b=2; t.c=3; t.a=nil; t.d=4      -- "all keys must be live when table is grown"
```

Both are deterministic. In the wasm build a single such sketch poisons the engine
instance: every later call on it throws `recursive use of an object detected`.
Constructing a fresh `PpuCore` would recover, but the studio never does —
`web/src/ppu/instance.ts` holds one `ppuCore`, documented "set exactly once
before first render — not for runtime swapping". So a published toy renders as a
dead canvas for every visitor until the page is reloaded. Roughly half of
randomly generated table-manipulation programs hit one of the two.

The cause is one function. `set`'s grow path had two arms, and the map insert
that follows needs *two* things done first — dead keys purged, and capacity
reserved. Each arm did exactly one:

| arm | purges dead keys | reserves capacity | panic |
|---|---|---|---|
| array-growth | yes, via `map.retain` | no | `map slot must be pre-reserved` |
| map-growth | no | yes | `all keys must be live when table is grown` |

`reserve_map` already did both, which is why upstream's fix routes the grow path
through it.

## What was changed

Everything is confined to `src/table/raw.rs` except one mechanical line.

1. **`RawTable::set` and the new `RawTable::grow_array`** — copied verbatim from
   piccolo master commit [`f617091a`](https://github.com/kyren/piccolo/commit/f617091a)
   ("A lot of improvements to RawTable", 2024-07-02). Not a locally invented
   restructuring: the maintainer's own fix, transplanted byte for byte.
2. **`src/string.rs:55`** — `(*ptr).len()` to `(&*ptr).len()`. Identical codegen:
   `.len()` takes `&self`, so rustc was already inserting the reborrow. Needed
   because `dangerous_implicit_autorefs` is deny-by-default for a path dependency,
   where rustc does not cap lints the way it does for a registry dependency — so
   this is a hard compile error when vendored, not a warning.
3. **`Cargo.toml`** — an empty `[workspace]` table so `cargo test` works inside
   this directory without the crate joining the ppu.toys workspace, and a
   `[lints.rust]` table allowing two lints (three warning sites). rustc caps lints
   for a registry dependency but not for a path one, so vendoring surfaces
   warnings upstream never had to fix. Both are form lints, not correctness ones,
   and both are named explicitly rather than blanket-allowed:
   `irrefutable_let_patterns` fires on one arm of the `impl_int_from!` macro
   expansion (`src/conversion.rs:221`, irrefutable only for `i64`->`i64`, where
   `TryFrom::Error = Infallible`; the `else` arm is live and correct for the other
   seven types), and `mismatched_lifetime_syntaxes` on two elided-lifetime returns
   (`src/stack.rs:78`, `src/thread/thread.rs:206`). Kept in the manifest rather
   than in `src/` so the audit diff below stays confined to two files.

### What was deliberately NOT taken from `f617091a`

That commit is much wider than the fix. It also removed `reserve_array`, added
`RawTable::with_capacity`, rewrote `src/thread/vm.rs`'s `NewTable` to call it, and
reworked `src/stdlib/table.rs` around new `array()`/`array_mut()` accessors. **All
of that was left at 0.3.3.** This is the real partial backport, and it is safe:

Under 0.3.3's `vm.rs`, `reserve_array(n)` leaves `array.len() == 0` with
`capacity >= n`, which does not satisfy master's "array len equals capacity"
invariant that `with_capacity` establishes. The backported `set` never relies on
that invariant holding at construction — it re-establishes it itself by calling
`grow_array`, whose body is character-identical to the `resize(capacity)` +
`retain` block 0.3.3's array arm already ran. Reaching the array arm with
`Some(index_key)` requires having fallen through the `index < self.array.len()`
early return, so `index_key >= array.len()`, and the arm's own
`optimal_size > index_key` guard then makes `optimal_size - array.len() >= 1` —
no underflow — while `grow_array` resizes `len` to `capacity >= optimal_size >
index_key`, so the subsequent index is in bounds.

Upstream's later commit
[`a1c6e3a`](https://github.com/kyren/piccolo/commit/a1c6e3a) ("Properly handle
dead keys in `RawTable::length`", 2024-07-04) is also not taken — see below.

Every local delta carries a `LOCAL DELTA (PPU-97)` comment. To audit, diff against
the published crate and expect changes in exactly **two files** — `src/string.rs`
(one hunk) and `src/table/raw.rs` (all hunks inside `set` and `grow_array`) — plus
`Cargo.toml` and this `README.md`:

```sh
diff -r vendor/piccolo/src ~/.cargo/registry/src/*/piccolo-0.3.3/src
```

For byte provenance against the actual published artifact rather than cargo's
extracted copy, untar `~/.cargo/registry/cache/*/piccolo-0.3.3.crate` and diff
that instead.

### `length()` and the `#` operator

Upstream's `a1c6e3a` changed `length()`'s map-probing loop from `.is_some()` to
`.is_some_and(|(_, v)| !v.is_nil())`. That was **not** backported, to keep the
delta to the panic. It is a standalone commit, not a companion to the fix — do
not assume the two must move together.

It does interact with the new `set`, which is worth knowing about. The backported
`set` is more conservative about growing the array — it guards on
`optimal_size > index_key` rather than 0.3.3's `optimal_size > old_array_size` — so
array-candidate integer keys linger in the map part in cases where 0.3.3 would have
migrated them into the array. That makes `length()`'s value-blind doubling loop run
more often than it used to.

It is still not observable as incorrectness, and 0.3.3's form is the *more*
conservative of the two: `.is_some()` overshoots past dead keys where
`.is_some_and(!nil)` stops at them. Either way the loop exits on a key that reads
nil, which is exactly the binary search's precondition, and the search predicate
below it is already `v.is_nil()`-aware, so the overshoot is absorbed. Measured over
4000 pseudo-random table programs: `#` is exact on hole-free sequences (grow and
shrink, n = 0..200), every border returned satisfies
`(n == 0 or t[n] ~= nil) and t[n+1] == nil`, and table *contents* are identical to
both stock 0.3.3 and piccolo master. What differs is *which* valid border comes back
for a sparse table — from stock in 217 of 2055 comparable programs, and from master
in 16 of 4000. Lua leaves that unspecified, so this is a legal difference, not a
defect. Backport `a1c6e3a` if it ever needs to match master exactly.

**The engine itself calls `length()`**, which is the first thing to re-check when
this directory is deleted: `crates/ppu-core/src/lua.rs:240` (`hk.length()` then
`for idx in 1..=n` over `__ppu_hooks`) and `:522` (`hooks.set(ctx, n + 1, entry)`).
Both are safe today because `__ppu_hooks` is a fresh table each frame, appended to
as a dense `1..n` sequence with no holes, so `#` is exact. `crates/ppu-core/tests/lua_table_grow.rs`
pins that hole-free exactness so the guarantee outlives this directory.

Trimmed from the published crate, since a `[patch]` dependency never builds them —
the full list, so the audit diff above reconciles: `examples/`, `tests/` fixtures
aside, `flake.nix`, `flake.lock`, `Cargo.lock`, `Cargo.toml.orig`,
`.cargo_vcs_info.json`, `CHANGELOG.md`, `COMPATIBILITY.md`, `.circleci/`, `.envrc`,
`.gitignore`, `.gitmodules`, and upstream's `README.md` (replaced by this file).
`tests/` itself is kept so the crate's own suite still runs, as are both
`LICENSE-CC0` and `LICENSE-MIT`. No build script, proc macro, or `include!` exists
anywhere in `src/`.

## Why vendored rather than a version bump or a fork

The fix landed on master 16 days after 0.3.3 shipped and has never been released.

- piccolo 0.3.3 (2024-06-16) is the last release; last commit on master is
  2025-07-10, and [issue #144](https://github.com/kyren/piccolo/issues/144),
  which is this exact panic, has been open since 2026-05-22 with no maintainer
  reply. Waiting for a release is not a plan.
- Pointing `[patch.crates-io]` at master by git rev would drag two years of VM
  change — stdlib, opcodes, thread, table — plus a git-only `gc-arena` revision,
  and needs the network at build time including in wasm CI.
- A GitHub fork is a repo we own forever plus that same network dependency.

Vendoring is a directory and three lines of `Cargo.toml`, hermetic and offline.

## Delete this directory when either happens

1. **The ottavino migration lands.** `ottavino` (crates.io, lumen-oss) is a
   maintained fork of piccolo master and already carries this fix. It is the
   intended destination; it was not done here because master renamed the error
   API `crates/ppu-core/src/lua.rs` depends on (`StaticError` to `ExternError`,
   `PrototypeError` to `CompilerError`) — that is what feeds the editor's gutter
   line numbers — and lands enough VM change to need a golden re-baseline.
2. **Upstream publishes any release after 0.3.3.** Then bump the dependency and
   drop the `[patch.crates-io]` stanza from the workspace `Cargo.toml`.

Either way the check is the same: remove the patch stanza and confirm
`crates/ppu-core/tests/lua_table_grow.rs` still passes.
