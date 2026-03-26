---
name: SSRF via unvalidated dynamic URI rendering
description: Dynamic gRPC URI rendered from untrusted event fields with no allowlist or scheme restriction, enabling SSRF
type: finding
status: resolved
priority: p1
issue_id: "001"
tags: [code-review, security, opentelemetry, grpc]
---

## Problem Statement

When `uri` is a template (e.g. `http://{{ host }}:4317`), the rendered value is taken verbatim from the event field and only validated as a syntactically valid `Uri`. There is no allowlist, no scheme restriction, and no hostname restriction applied to the rendered result.

An attacker who controls event data (e.g. via a log ingestion pipeline that accepts external input) can inject any value into the `host` field, directing Vector to make gRPC TCP connections to arbitrary internal network hosts including cloud metadata endpoints (`169.254.169.254`), internal Kubernetes services, etc.

The same primitive applies to dynamic header values — while constrained to ASCII, they are not restricted beyond that type check.

## Findings

- **File**: `src/sinks/opentelemetry/grpc.rs` lines 711-736
- **Mechanism**: `uri_template.render_string(&event)` → `rendered.parse::<Uri>()` → used directly for connection
- **Documented use case**: per-tenant routing with `{{ host }}` — explicitly designed for scenarios where event data controls egress destination
- **No allowlist or scheme restriction in the URI validation path**

## Proposed Solutions

### Option A: URI allowlist config key
Add `allowed_uri_prefixes: Vec<String>` to `GrpcSinkConfig`. Any rendered URI that doesn't match a prefix is dropped with `SinkRequestBuildError`. Fail closed.
- **Pros**: Flexible, operator-controlled, explicit
- **Cons**: Breaking addition to config, operators must configure it to use dynamic URIs
- **Effort**: Medium
- **Risk**: Low (additive)

### Option B: Restrict dynamic URI to scheme + host changes only
Parse the rendered URI and validate scheme is `http` or `https` only, and optionally restrict port range. Reject file://, ftp://, etc.
- **Pros**: Simpler than allowlist, catches obvious SSRF vectors
- **Cons**: Doesn't prevent SSRF to internal network; `http://169.254.169.254:80` still valid
- **Effort**: Small
- **Risk**: Low

### Option C: Document as operator responsibility, add security warning to config docs
Add a `#[configurable(metadata(docs::warnings = "..."))]` annotation noting that dynamic URI templates should only be used with trusted event data.
- **Pros**: No code change
- **Cons**: Does not prevent the vulnerability; just warns
- **Effort**: Small
- **Risk**: None

## Acceptance Criteria

- [x] Dynamic URI rendering is either restricted (allowlist) or clearly documented as requiring trusted input
- [x] At minimum, scheme is validated to be `http` or `https` only for rendered URIs
- [x] Any rendered URI that fails validation emits `SinkRequestBuildError` and drops the event

## Work Log

- 2026-03-26: Identified by security-sentinel review agent
