---
title: fallible BBG storage consumer migration
tags: evy, bbg, storage, audit
date: 2026-09-11
status: scoped-validation
---

# Fallible storage consumer migration

Evy's ShardStorage now propagates BBG errors from put, dirty marking, remove and
commit. Owned typed read and bounded scan expose persisted access while get,
get_raw, get_mut_raw and iter remain cache access. The BBG change identity
returned by commit has its storage meaning; authenticated state roots remain
BBG's responsibility.

DispatchNode returns a result. The scheduler stops at an error and releases its
borrowed engine-pool references. Engine::run_frame returns that error without
advancing the frame counter. Engine::commit_tick propagates the original storage
error, including CommitUnknown, without advancing the tick counter. All local
implementations and the hello example use the result. Earlier staged mutations
remain the caller's recovery responsibility.

The existing namespace test expected the old ephemeral dimension number 12.
The production enum already referenced BBG's constant correctly. Its assertion
now expects dimension 13, keeping dimension 12 reserved for BBG intents.

## Validation

The normal command was attempted:

```sh
cargo test -p evy_ecs_storage --locked --no-default-features
```

Cargo stops during resolution because Evy's existing backend-unimem feature
forwards bbg/backend-unimem, which BBG does not expose. This fails even with
default features disabled. Repository Cargo manifests and Cargo.lock were
preserved.

An isolated workspace pointed each package's library and example directly at
the source files in this repository. It copied the eight necessary Evy package
manifests into a temporary directory and omitted the unavailable unimem feature
forwarding edge there. It used the real sibling BBG and honeycrisp dependencies.
This checks the selected default-feature source configuration; it does not
establish that the original full workspace resolves or that unimem works.

Reproduce from the Evy repository with Nushell:

```nu
let verification = (nu audit/verify-storage-consumer.nu | str trim)
cargo test --offline --manifest-path ($verification | path join Cargo.toml) -p evy_ecs_storage -p evy_engine_dispatch -p evy_engine_core --all-targets
```

The executed workspace was `/tmp/evy-fallible.3pLdvR`. After its initial dependency
resolution, the final command also used `--locked`. On macOS arm64 it passed, at
branch content c29fed9e29914105f69fbfc9769b2da50ee06474 — the commit that
introduced both this change and this page:

| Package | Unit tests |
|---|---:|
| evy_ecs_storage | 27 |
| evy_engine_dispatch | 18 |
| evy_engine_core | 11 |

The hello example compiled as a test target. Four new regressions cover:

- Backend I/O errors remain distinguishable from absent data and success;
  CommitUnknown retains its change identity and message.
- Typed owned reads and scans preserve element bounds and the exclusive cursor.
- Unknown commit outcome reaches the engine caller without incrementing tick.
- Failed dispatch stops subsequent nodes and does not increment frame.

Existing dependency warnings remain. This evidence covers consumer API
propagation; disk recovery and native Cyber node persistence belong to
BBG/Cybergraph's separate acceptance gates.

The review-close commits that follow c29fed9 add a dispatch test for the pool
release on the error path, forward `read` and `scan` in the engine test double,
move the scheduler test module into `scheduler_tests.rs`, and add the §6.1 spec
listing and the re-exports in both crates' `lib.rs`, so the table above no
longer counts the head. That delta was not re-run: the working clones carry no
sibling `bbg`, `honeycrisp`, `hemera` or `strata` checkout, so `bbg = { path =
"../bbg/rs" }` does not resolve there and neither the workspace nor
`verify-storage-consumer.nu` can build.

No gate judges this head either: `ci`, `build` and `use` are all off for this
repository. The merge candidate is therefore unverified — nothing has compiled
the head commit, and the counts above belong to c29fed9 alone. Whoever takes it
should run the two commands of the Reproduce block in a checkout that holds the
sibling `bbg`, `honeycrisp`, `hemera` and `strata` repositories and record the
head hash beside the counts.
