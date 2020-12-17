# [Vector](https://vector.dev) Helm Chart

This is an opinionated Helm Chart for running [Vector](https://vector.dev) in Kubernetes.

Our charts use Helm's dependency system, however we only use local dependencies.
Head over to the repo for [more information on development and contribution](https://github.com/timberio/vector/tree/master/distribution/helm).

The agent role is designed to collect all Kubernetes log data on each Node. Vector runs as a [DaemonSet](https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/) and tails logs for the entire Pod, automatically enriching them with Kubernetes metadata via the [Kubernetes API](https://kubernetes.io/docs/concepts/overview/kubernetes-api/). Collection is handled automatically, and it is intended for you to adjust your pipeline as necessary using Vector's [sources](https://vector.dev/docs/reference/sources/), [transforms](https://vector.dev/docs/reference/transforms/), and [sinks](https://vector.dev/docs/reference/sinks/).

To get started add the Helm chart repo

```
helm repo add timberio https://packages.timber.io/helm/latest
```

Check the available Helm chart configuration options

```
helm show values timberio/vector-agent
```

Set up a Vector config that leverages our `kubernetes_logs` data source

```
cat <<-'VALUES' > values.yaml
# The Vector Kubernetes integration automatically defines a
# kubernetes_logs source that is made available to you.
# You do not need to define a log source.
sinks:
  # Adjust as necessary. By default we use the console sink
  # to print all data. This allows you to see Vector working.
  # https://vector.dev/docs/reference/sinks/
  stdout:
    type: console
    inputs: ["kubernetes_logs"]
    rawConfig: |
      target = "stdout"
      encoding = "json"
VALUES
```

To install the chart

```
helm install --namespace vector --create-namespace vector timberio/vector-agent --values values.yaml
```

To update

```
helm install --namespace vector --create-namespace vector timberio/vector-agent --values values.yaml
```
