---
name: TLS mode fixed at build time silently sends plaintext for some dynamic URI templates
description: use_https flag computed once at build time; templates like {{ scheme }}://{{ host }} may silently connect over plaintext
type: finding
status: resolved
priority: p1
issue_id: "002"
tags: [code-review, security, tls, opentelemetry, grpc]
---

## Problem Statement

The `use_https` flag is computed once at sink build time:

```rust
let use_https = self.tls.is_some()
    || static_uri.as_ref().is_some_and(|u| u.scheme_str() == Some("https"))
    || self.uri.get_ref().starts_with("https://");
```

The third check (`starts_with("https://")`) only matches when the literal template string begins with `https://`. A template like `{{ scheme }}://{{ host }}:4317` or even `http://{{ host }}:4317` where an event renders to `https://...` will have `use_https = false`. The underlying Hyper client is then built with `MaybeTlsSettings::Raw(())` — a plaintext connector — regardless of the rendered URI scheme.

This means operators using fully-dynamic URI templates who believe they have TLS will silently send telemetry in plaintext.

## Findings

- **File**: `src/sinks/opentelemetry/grpc.rs` lines 155-169
- **Risk**: Silent plaintext egress for PII/confidential telemetry
- **Affected configurations**: Any dynamic URI template where the scheme is not a literal static prefix

## Proposed Solutions

### Option A: Validate rendered URI scheme matches build-time TLS mode
At event render time, check that the rendered URI's scheme matches `use_https`. If they conflict, drop the event with a `SinkRequestBuildError` and a clear message.
- **Pros**: Fails loudly, no silent plaintext
- **Cons**: Drops events when config is ambiguous; operator must configure `tls:` block or use static scheme
- **Effort**: Small
- **Risk**: Low

### Option B: Require scheme to be a static literal in dynamic URI templates
At config parse time, validate that the scheme portion of any template URI is not itself a template expression. Reject configs like `{{ scheme }}://{{ host }}`.
- **Pros**: Eliminates the ambiguity at startup
- **Cons**: Restricts valid template patterns
- **Effort**: Medium (requires template parsing)
- **Risk**: Low

### Option C: Document the limitation clearly in config schema
Add a docstring note to the `uri` field: "When using a dynamic template that renders to `https://`, you must also configure `tls:` to ensure TLS is used. The sink cannot infer TLS from a fully-dynamic scheme."
- **Pros**: No code change, low effort
- **Cons**: Doesn't prevent the issue, just warns
- **Effort**: Small
- **Risk**: None

## Acceptance Criteria

- [x] Operators are not silently sent over plaintext when they configure a dynamic `https://` URI template
- [x] Either the scheme mismatch is detected and rejected at runtime, or config validation prevents it

## Work Log

- 2026-03-26: Identified by security-sentinel review agent
