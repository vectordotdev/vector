# RFC 10298 - 2021-12-06 - `kubernetes_logs` Rewrite

Rewriting the `kubernetes_logs` source to leverage the "official" Rust client for Kubernetes.

## Context

- [`kubernetes_logs` error handling RFC](https://github.com/vectordotdev/vector/issues/7527)
- [`kubernetes_logs` source fixes](https://github.com/vectordotdev/vector/issues/10016)
- [Extracting Kubernetes runtime out of Vector code base](https://github.com/vectordotdev/vector/issues/2963)

## Scope

### In scope

- Replacing our in-house Kubernetes code with `kube` wherever possible

### Out of scope

- Changing the end-to-end functionality of the `kubernetes_logs` source
- Changing the testing strategies we have around the `kubernetes_logs` source today

## Pain

- Difficulty troubleshooting and supporting this feature as maintainers
- Unresolved bug reports from users related to our in-house Kubernetes code

## Proposal

### User Experience

This change should not affect our UX or existing deployments using the `kubernetes_logs`
source today. Any changes will be entirely internal to the source's implementation.

### Implementation

Our existing implementation relies on many primitives from [kube](https://docs.rs/kube/*/kube/)
and we have the opportunity to leverage the higher level tools provided by the
library as well.

`kube` is the leading Rust client for Kubernetes and we should look to utilize
the community support and experience behind it as much as possible. We can replace
most of our "plumbing" level code with the equivalent, or high level, code from
`kube` while keeping the same functionality.

As far as I'm aware the only implementation we need to keep in-house is the `Store`
as we want to retain the contents in the `Store` after receiving a DELETE event for
the corresponding contents. This allows us to enrich events we receive from a Pod
after it's been deleted but we still have an open file handle for it's logs.

Some of our existing code in `src/kubernetes` is already taken directly from `kube`
(an older version) without modification which lends itself to being replaced completely.

## Rationale

Rewriting the `kubernetes_logs` source to leverage the existing "official" Kubernetes
client for Rust will give us a more stable and maintainable foundation. We currently
are unable to maintain and troubleshoot the `kubernetes_logs` source properly and
leveraging `kube` directly will provide an existing community of users, tests, and
experiences to improve our ability to support this feature in Vector.

Relying on an external library will reduce the amount of code and complexity that
currently exists within Vector and allow us to focus less on making to tools correct
and more on ensuring we're using them properly.

### Existing crate vs our implementation

Leveraging `kube` was considered at the start of the Kubernetes integration project,
but eventually we wrote our own implementation on top of the `k8s-openapi` crate.

Over the past year and a half, `kube` has matured greatly and today it is being
[donated](https://github.com/kube-rs/kube-rs/issues/584) to the [CNCF](https://www.cncf.io/).
This appears to be the [state of `kube` roughly at the time](https://github.com/kube-rs/kube-rs/tree/c38d82162d2626bcfcf2ef8cf0c9d93e0734af49)
of writing our own implementation. While our existing implementation is quite
generic and modular, the needs of the component today are quite limited.
Realistically we just need the following:

- Client: authentication and configuration to call the Kubernetes API
  - `kube-client`
- Reflector: error handling and a persistent cache for an event stream
  - `kube-runtime::reflector`

## Drawbacks

Converting the underlying libraries to `kube` (where possible) doesn't guarantee
a resolution for reported bugs, but it does shift a non-trivial amount of functionality
out of our project and onto a specialized library.

## Prior Art

- [Vector `kubernetes_logs` source](https://github.com/vectordotdev/vector/tree/master/src/sources/kubernetes_logs)
  - [Vector `kubernetes`](https://github.com/vectordotdev/vector/tree/master/src/kubernetes)
- [Datadog Kubelet provider](https://github.com/DataDog/datadog-agent/blob/main/pkg/autodiscovery/providers/kubelet.go)
- [Fluent-Bit Kubernetes filter](https://github.com/fluent/fluent-bit/tree/master/plugins/filter_kubernetes)

## Alternatives

### Not do this at all

Troubleshooting and maintaining the existing source has been challenging, and there is
a clear lack of understanding of the current implementation. The source is an important
part of our OSS offering and frequently used with the Agent role. Not doing this impairs
our ability to support Vector running as an Agent in Kubernetes and reflects poorly on
our overall reputation.

### Rewrite to integrate with the Kubelet API

This avenue was recommended internally, but is a departure from our existing implementation,
and thus contains more unknowns and increases the likelihood of breaking changes to the
existing source.

### Replace existing source with an enrichment transform

This option is capable of being used regardless of what role Vector has been deployed
in, but it is a larger change to implement and a larger update for users too. Longer
term the source can be decoupled and introduced as a standalone enrichment component.

## Outstanding Questions

- ~~Do we want to re-evaluate the usage of `evmap` in this component?~~ Not critical,
we can review performance later.
- ~~Do we want to make this change with a version change of the source, opting in
to the change with a `version` option in the configuration?~~ No, the risk seems low
enough to not warrant a version split between the old and new code.

## Plan Of Attack

- Replace contents of `src/kubernetes` with equivalents from `kube`
  1. `src/kubernetes/client` replaced with `kube-client`
  1. `src/kubernetes/reflector` and dependencies replaced with `kube-runtime::reflector`
  1. `src/kubernetes/state` updates to minimize in-house code
- Ensure unit tests and integration tests show matching behavior before and after rewrite

## Future Improvements

- Move Kubernetes enrichment to a standalone transform and rework source to be
a more simple combination of file source plus the new enrichment transform
- Rewrite kubernetes_logs source to integrate with Kubelet API to reduce calls
to the Kubernetes Control Plane.
