Upgraded `kube` to 4.2.1 to fix [RUSTSEC-2026-0600](https://rustsec.org/advisories/RUSTSEC-2026-0600). The `kubernetes_logs` source now trusts only the configured cluster CA (from the kubeconfig or the in-cluster `ca.crt`) when connecting to the Kubernetes API server, instead of also trusting OpenSSL's default certificate store. If your API server or an `https://` proxy relied on the system trust store, add its CA to the kubeconfig CA bundle.

authors: JinchengCao
