---
date: "2022-03-28"
title: "`kube` for `kubernetes_logs`"
description: "The `kubernetes_logs` source is now powered by the `kube-rs` library"
authors: ["spencergilbert"]
pr_numbers: [11714]
release: "0.21.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Our `kubernetes_logs` source has been updated to use [`kube-rs`](https://kube.rs/)
as the foundations of our integration. `kube-rs` is a CNCF Sandbox Project and
has served as the basis of a number of applications that work with the Kubernetes API.
We made this change to improve the reliability and stability of our `kubernetes_logs`
source and to ensure our compatibility with the Kubernetes API as it evolves.

There are two user facing changes for this update: the `list` verb permission is
now required for Vector's ClusterRole within the Kubernetes cluster, and the [`proxy`](https://vector.dev/docs/reference/configuration/global-options/#proxy)
options are no longer used for the client.

The [`vector` Helm chart](https://github.com/vectordotdev/helm-charts/tree/develop/charts/vector)
has been updated to include this new verb as of `0.7.0`, and the Kustomize based
manifests in the `vector` repo have been updated as part of the `0.21.0` release.
If you are managing your own manifests separately you need to ensure the ClusterRole
used by Vector includes both the `list` and `watch` verbs for `pods` and `namespaces`.

A custom `kubeconfig` should now be provided to leverage any internal proxy you
use to interact with the Kubernetes API, rather than the `proxy` option previously
available to this source.
