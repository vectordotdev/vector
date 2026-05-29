---
sut_path: /home/ssm-user/src/vector
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
external_references:
  - path: lib/vector-buffers/
    why: Scanned entire crate (and whole repo) for Antithesis SDK imports and assertion calls
---

# Existing Antithesis SDK Assertions

## Summary

**No Antithesis SDK instrumentation exists anywhere in the Vector codebase.**

A repo-wide scan for the Antithesis SDK and its assertion macros/functions found
zero matches.

## Scan Performed

```
grep -rn "antithesis" --include="*.rs" --include="*.toml"        # repo-wide: 0 matches
grep -rn "assert_always|assert_sometimes|assert_reachable|assert_unreachable|antithesis_sdk" \
     --include="*.rs" lib/vector-buffers/                          # 0 matches
```

- No `antithesis-sdk` (or any `antithesis*`) dependency in any `Cargo.toml`.
- No imports of an Antithesis SDK crate.
- No calls to `assert_always!`, `assert_sometimes!`, `assert_reachable!`,
  `assert_unreachable!`, or their non-macro equivalents.

## Implication for Property Discovery

Every property in the catalog starts from zero instrumentation. All SUT-side
assertion suggestions in the evidence files are **missing** (not partial, not
already-present) and must be added if adopted. The codebase does, however, make
heavy use of:

- `tracing` (`trace!`/`debug!`/`error!`) — useful as anchor points for where
  Antithesis assertions would naturally sit.
- `metrics`-based internal events (`lib/vector-buffers/src/internal_events.rs`,
  `buffer_usage_data.rs`) — these are the existing observability surface; several
  known bugs (see external references) live precisely in the gap between these
  metrics and reality.
- `debug_assert!` / `assert!` in a few hot paths and extensive `proptest` +
  model-based tests under `variants/disk_v2/tests/` — these indicate where the
  authors already considered invariants worth checking.

## Assumptions / Open Questions

- Assumption: the workload and any SUT-side assertions will be added fresh under
  this Antithesis effort. The deployment topology must include the Antithesis
  Rust SDK as a new dependency for any SUT-side instrumentation.
</content>

</invoke>
