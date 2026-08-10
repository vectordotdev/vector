---
what: "Stable Vector artifacts hosted at `packages.timber.io`"
deprecated_since: "0.58.0"
---

Stable Vector release archives and packages are now available from
[GitHub Releases](https://github.com/vectordotdev/vector/releases). The
`packages.timber.io/vector` URLs are deprecated for stable releases.

For a pinned version, use the corresponding GitHub Release asset instead of a
direct `packages.timber.io` URL. For example, download the Linux x86_64 GNU
archive for version `0.58.0` from:

```text
https://github.com/vectordotdev/vector/releases/download/v0.58.0/vector-0.58.0-x86_64-unknown-linux-gnu.tar.gz
```

The Vector installer and the download instructions on
[vector.dev](https://vector.dev/download/) use GitHub Releases for stable
artifacts. Existing URLs continue to work during the deprecation period.

| Artifact | Migration outcome |
| --- | --- |
| Stable Vector releases, installers, and package-manager downloads | [GitHub Releases](https://github.com/vectordotdev/vector/releases) |
| `packages.timber.io/vector` versioned, `latest`, and version-alias URLs | Removed after the deprecation period |
| Nightly builds | Will be deleted; not migrated |
| Custom builds | Will be deleted; not migrated |
| Helm charts | [GitHub Release assets](https://github.com/vectordotdev/helm-charts/releases); `https://helm.vector.dev` remains the Helm repository index |
| Legacy Timber agent, CLI, and CloudWatch Lambda artifacts | Will be deleted; not migrated |
