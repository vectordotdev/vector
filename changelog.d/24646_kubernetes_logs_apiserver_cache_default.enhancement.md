The `kubernetes_logs` source now defaults `use_apiserver_cache` to `true`. This allows the kube-apiserver to serve LIST requests from its watch cache instead of issuing quorum reads directly against etcd, which can overload etcd in large clusters. Users who require strongly consistent reads can restore the previous behavior by explicitly setting `use_apiserver_cache: false`.

authors: NierYYDS
