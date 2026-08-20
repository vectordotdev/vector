---
what: "Stable Vector artifacts hosted at `packages.timber.io`"
deprecated_since: "0.58.0"
---

Stable Vector release archives and packages are now hosted in the public COSE
release bucket. The `packages.timber.io/vector` URLs are deprecated.

For a pinned version, replace the hostname while preserving the version and
filename. For example, download the Linux x86_64 GNU archive for version
`0.58.0` from:

```text
https://dd-cose-releases.s3.amazonaws.com/vector/0.58.0/vector-0.58.0-x86_64-unknown-linux-gnu.tar.gz
```

The Vector installer and the download instructions on
[vector.dev](https://vector.dev/download/) use the COSE bucket. Existing
`packages.timber.io` URLs continue to work during the deprecation period.

| Artifact | Migration outcome |
| --- | --- |
| Stable Vector releases, installers, and package-manager downloads | Migrated to `dd-cose-releases` under `vector/<version>/` |
| `packages.timber.io/vector` versioned, `latest`, and version-alias URLs | Removed after the deprecation period |
| Existing nightly and custom builds | Will be deleted; not migrated |
| Future nightly and custom builds | Published to `dd-cose-releases` under `vector/nightly/` and `vector/custom/` |
| Helm charts | [GitHub Release assets](https://github.com/vectordotdev/helm-charts/releases); `https://helm.vector.dev` remains the Helm repository index |
| Legacy Timber agent, CLI, and CloudWatch Lambda artifacts | Will be deleted; not migrated |
