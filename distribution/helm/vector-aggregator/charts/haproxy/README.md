# ![HAProxy](https://github.com/haproxytech/kubernetes-ingress/raw/master/assets/images/haproxy-weblogo-210x49.png "HAProxy")

## HAProxy Helm Chart

## Introduction

This chart bootstraps an HAProxy load balancer as deployment/daemonset on a [Kubernetes](http://kubernetes.io) cluster using the [Helm](https://helm.sh) package manager. As oposed to [HAProxy Kubernetes Ingress Controller](https://github.com/haproxytech/helm-charts/tree/main/kubernetes-ingress) Chart, HAProxy is installed as a regular application and not as an Ingress Controller.

### Prerequisites

- Kubernetes 1.12+
- Helm 2.9+

## Before you begin

### Setup a Kubernetes Cluster

The quickest way to setup a Kubernetes cluster is with [Azure Kubernetes Service](https://azure.microsoft.com/en-us/services/kubernetes-service/), [AWS Elastic Kubernetes Service](https://aws.amazon.com/eks/) or [Google Kubernetes Engine](https://cloud.google.com/kubernetes-engine/) using their respective quick-start guides.

For setting up Kubernetes on other cloud platforms or bare-metal servers refer to the Kubernetes [getting started guide](http://kubernetes.io/docs/getting-started-guides/).

### Install Helm

Get the latest [Helm release](https://github.com/helm/helm#install).

### Add Helm chart repo

Once you have Helm installed, add the repo as follows:

```console
helm repo add haproxytech https://haproxytech.github.io/helm-charts
helm repo update
```

## Install the chart

To install the chart with Helm v3 as _my-release_ deployment:

```console
helm install my-release haproxytech/haproxy
```

**_NOTE_**: To install the chart with Helm v2 (legacy Helm) the syntax requires adding deployment name to `--name` parameter:

```console
helm install haproxytech/haproxy \
  --name my-release
```

### Installing with unique name

To auto-generate resource names when installing, use the following:

```console
helm install haproxytech/haproxy \
  --generate-name
```

### Installing from a private registry

To install the chart using a private registry for HAProxy (for instance to use a HAPEE image) into a separate namespace _prod_.

**_NOTE_**: Helm v3 requires namespace to be precreated (eg. with `kubectl create namespace prod`)

```console
helm install my-haproxy haproxytech/haproxy  \
  --namespace prod \
  --set image.tag=latest \
  --set image.repository=myregistry.domain.com/imagename \
  --set imageCredentials.registry=myregistry.domain.com \
  --set imageCredentials.username=MYUSERNAME \
  --set imageCredentials.password=MYPASSWORD
```

Alternatively, use a pre-configured (existing) imagePullSecret in the same namespace:

```console
helm install my-ingress haproxytech/haproxy  \
  --namespace prod \
  --set image.tag=SOMETAG \
  --set existingImagePullSecret name-of-existing-image-pull-secret
```

### Installing as DaemonSet

Default image mode is [Deployment](https://kubernetes.io/docs/concepts/workloads/controllers/deployment/), but it is possible to use [DaemonSet](https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/) as well:

```console
helm install my-haproxy2 haproxytech/haproxy \
  --set kind=DaemonSet
```

**_NOTE_**: With helm `--set` it is needed to put quotes and escape dots in the annotation key and commas in the value string.

### Installing with Horizontal Pod Autoscaler

[HPA](https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/) automatically scales number of replicas in Deployment or Replication Controller and adjusts replica count. Therefore we want to unset default replicaCount by setting corresponding key value to null and enable autoscaling:

```console
helm install my-haproxy3 haproxytech/haproxy \
  --set kind=Deployment \
  --set replicaCount=null \
  --set autoscaling.enabled=true \
  --set autoscaling.targetCPUUtilizationPercentage=80
```

**_NOTE_**: Make sure to look into other tunable values for HPA documented in [values.yaml](values.yaml).

### Installing with service annotations

On some environments like EKS and GKE there might be a need to pass service annotations. Syntax can become a little tedious however:

```console
helm install my-haproxy4 haproxytech/haproxy \
  --set kind=DaemonSet \
  --set service.type=LoadBalancer \
  --set service.annotations."service\.beta\.kubernetes\.io/aws-load-balancer-internal"="0.0.0.0/0" \
  --set service.annotations."service\.beta\.kubernetes\.io/aws-load-balancer-cross-zone-load-balancing-enabled"="true"
```

**_NOTE_**: With helm `--set` it is needed to put quotes and escape dots in the annotation key and commas in the value string.

### Using values from YAML file

As opposed to using many `--set` invocations, much simpler approach is to define value overrides in a separate YAML file and specify them when invoking Helm.
The `config` block can also support using helm templates to populate dynamic values, e.g. `{{ .Release.Name }}`.

_mylb.yaml_:

```yaml
kind: DaemonSet
config: |
  global
    log stdout format raw local0
    daemon
    maxconn 1024
  defaults
    log global
    timeout client 60s
    timeout connect 60s
    timeout server {{ .Values.global.serverTimeout }}
  frontend fe_main
    bind :80
    default_backend be_main
  backend be_main
    server web1 10.0.0.1:8080 check
    server web2 {{ .Release.Name }}-web:8080 check
service:
  type: LoadBalancer
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-cross-zone-load-balancing-enabled: "true"
    service.beta.kubernetes.io/aws-load-balancer-internal: 0.0.0.0/0
```

And invoking Helm becomes (compare to the previous example):

```console
helm install my-haproxy5 -f mylb.yml haproxytech/haproxy
```

### Using secrets in additional volume mounts

In order to e.g. support SSL certificates, you can mount additional volumes from secrets:

_mylb.yaml_:

```yaml
service:
  type: LoadBalancer
config: |
  global
    log stdout format raw local0
    daemon
    maxconn 1024
  defaults
    log global
    timeout client 60s
    timeout connect 60s
    timeout server 60s
  frontend fe_main
    mode http
    bind :80
    bind :443 ssl crt /usr/local/etc/ssl/tls.crt
    http-request redirect scheme https code 301 unless { ssl_fc }
    default_backend be_main
  backend be_main
    mode http
    server web1 10.0.0.1:8080 check
mountedSecrets:
  - volumeName: ssl-certificate
    secretName: star-example-com
    mountPath: /usr/local/etc/ssl
```

The above example assumes that there is a certificate in key `tls.crt` of a secret called `star-example-com`.

### Using additional volumes and volumeMounts

In order to load data from other sources (e.g. to preload something inside an init-container) you can mount additional volumes to the container:

```yaml
extraVolumes:
  - name: tls
    emptyDir: {}
  - name: tmp
    emptyDir:
      medium: Memory

extraVolumeMounts:
  - name: tls
    mountPath: /etc/tls
  - name: tmp
    mountPath: /tmp
```

## Installing as non-root with binding to privileged ports

To be able to bind to privileged ports such as tcp/80 and tcp/443 without root privileges (UID and GID are set to 1000 in the example, as HAProxy Docker image has UID/GID of 1000 reserved for HAProxy), there is a special workaround required as `NET_BIND_SERVICE` capability is [not propagated](https://github.com/kubernetes/kubernetes/issues/56374), so we need to use `initContainers` feature as well:

```yaml
kind: DaemonSet
containerPorts:
  http: 80
  https: 443
  stat: 1024
daemonset:
  useHostNetwork: true
  useHostPort: true
  hostPorts:
    http: 80
    https: 443
    stat: 1024
config: |
  global
    log stdout format raw local0
    maxconn 1024
  defaults
    log global
    timeout client 60s
    timeout connect 60s
    timeout server 60s
  frontend fe_main
    bind :80
    default_backend be_main
  backend be_main
    server web1 127.0.0.1:8080 check
securityContext:
  enabled: true
  runAsUser: 1000
  runAsGroup: 1000
initContainers:
  - name: sysctl
    image: "busybox:musl"
    command:
      - /bin/sh
      - -c
      - sysctl -w net.ipv4.ip_unprivileged_port_start=0
    securityContext:
      privileged: true
```

## Upgrading the chart

To upgrade the _my-release_ deployment:

```console
helm upgrade my-release haproxytech/haproxy
```

## Uninstalling the chart

To uninstall/delete the _my-release_ deployment:

```console
helm delete my-release
```

## Debugging

It is possible to generate a set of YAML files for testing/debugging:

```console
helm install my-release haproxytech/haproxy \
  --debug \
  --dry-run
```

## Contributing

We welcome all contributions. Please refer to [guidelines](../CONTRIBUTING.md) on how to make a contribution.
