---
what: "Stable Vector artifacts hosted at `packages.timber.io`"
deprecated_since: "0.59.0"
---

Stable Vector release archives and packages are now hosted in the public COSE
release bucket. The `packages.timber.io/vector` URLs are deprecated and users should
switch to `install.datadoghq.com/vector` instead.

Starting with Vector `0.59.0`, the Vector installer and the download
instructions on [vector.dev](https://vector.dev/download/) point to artifacts
in the COSE bucket. Existing `packages.timber.io` URLs continue to work during
the deprecation period.

| Artifact | Migration outcome |
| --- | --- |
| Stable Vector releases, installers, and package-manager downloads | Migrated to exact-version paths under `install.datadoghq.com/vector/<version>/`; stable `latest` or `.X` aliases are not provided |
| `packages.timber.io/vector` versioned, `latest`, and version-alias URLs | Remain readable through December 31, 2026, and may be deleted at any time after that date |
| Nightly and custom builds | Treated as transient artifacts that are superseded by stable releases, so existing builds are not migrated; future builds are published to `install.datadoghq.com` under `vector/nightly/` and `vector/custom/` |
| Helm charts | [GitHub Release assets](https://github.com/vectordotdev/helm-charts/releases); `https://helm.vector.dev` remains the Helm repository index |
| All other legacy artifacts | Not migrated and will be deleted |
