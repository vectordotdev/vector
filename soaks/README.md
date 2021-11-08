# Soak Tests

This directory contains vector's soak tests, the integrated variant of our
benchmarks. The idea was first described in [RFC
6531](../rfcs/2021-02-23-6531-performance-testing.md) and has been steadily
improved as laid out in [Issue
9515](https://github.com/vectordotdev/vector/issues/9515).

## Index of Soaks

The test definitions are in `./tests`. `ls -1 ./tests` will give you an index of
available tests. Each test has its own README.md file with more details.

## Requirements

In order to run a soak locally you will need:

* at least 6 CPUs
* at least 6Gb of RAM
* minikube
* [miller](https://github.com/johnkerl/miller)
* docker
* terraform

The CPU and RAM requirements are currently hard-coded but might be made
flexible, possibly on a per-soak basis.

## Approach

The approach taken here is intentionally simplistic. A 'soak' is a defined set
of feature flags for building vector, a set of terraform to install vector and
support programs in a minikube and some glue code to observe vector in
operation. Consider this command:

```shell
> ./soaks/soak.sh --local-image --soak datadog_agent_remap_datadog_logs --baseline a32c7fd09978f76a3f1bd360c3a8d07a49538b70 --comparison be8ceafbf994d06f505bdd9fb392b00e0ba661f2
```

Here we run the soak test `datadog_agent_remap_datadog_logs` comparing vector at
`a32c7fd09978f76a3f1bd360c3a8d07a49538b70` with vector at
`be8ceafbf994d06f505bdd9fb392b00e0ba661f2`. Two vector containers will be built
for each SHA. The soak itself is defined in terraform, see
[`soaks/datadog_agent_remap_datadog_logs/terraform`].

After running this command you will, in about ten minutes depending on whether
you need to build containers or not, see a summary:

```shell
...
Apply complete! Resources: 16 added, 0 changed, 0 destroyed.
Recording 'comparison' captures to /tmp/datadog_agent_remap_datadog_logs-captures.ZSRFXO
~/projects/com/github/timberio/vector/soaks ~/projects/com/github/timberio/vector
✋  Stopping node "minikube"  ...
🛑  1 nodes stopped.
🔥  Deleting "minikube" in kvm2 ...
💀  Removed all traces of the "minikube" cluster.
~/projects/com/github/timberio/vector
Captures recorded to /tmp/datadog_agent_remap_datadog_logs-captures.ZSRFXO

Here is a statistical summary of that file. Units are bytes.
Higher numbers in the 'comparison' is better.

EXPERIMENT   SAMPLE_min       SAMPLE_p90       SAMPLE_p99       SAMPLE_max       SAMPLE_skewness  SAMPLE_kurtosis
baseline     24739333.118644  25918423.847458  26095157.813559  26160720.271186  -0.019132        -0.690881
comparison   35376407.491525  36809921.423729  36975943.016949  37141773.576271  -0.280115        -1.330509
```

The `baseline` experiment maps to the first SHA given to `soak.sh` --
`a32c7fd09978f76a3f1bd360c3a8d07a49538b70` -- and represents the starting point
of vector's throughput for this soak test. This baseline had a minimum observed
byte/second throughput of 24739333.118644/sec, a max of 26160720.271186/sec
etc. The comparison experiment has improved throughput -- higher numbers are
better -- even if the experiment was slightly more skewed than baseline and had
higher "tailedness". Improving this summary is a matter of importance.

## Defining Your Own Soak

Assuming you can follow the pattern of an existing soak test you _should_ be
able to define a soak by copying the relevant soak into a new directory and
updating the configuration that is present in that soak's terraform. Consider
the "Datadog Agent -> Remap -> Datadog Logs" soak in
[`tests/datadog_agent_remap_datadog_logs/`](tests/datadog_agent_remap_datadog_logs/). If you
`tree` that directory you'll see:

```shell
> tree tests/datadog_agent_remap_datadog_logs
tests/datadog_agent_remap_datadog_logs
├── README.md
└── terraform
    ├── http_blackhole.toml
    ├── http_gen.toml
    ├── main.tf
    ├── prometheus.tf
    ├── prometheus.yml
    ├── variables.tf
    └── vector.toml

1 directory, 9 files
```

The `terraform/` sub-directory contains a small project definition. It's clear
we can thin this out further -- the prometheus setup is common to all soaks --
but the primary things you need to concern yourself with are:

* `main.tf`
* `vector.toml`
* `http_blackhole.toml`
* `http_gen.toml`

The `main.tf` contents are:

```terraform
terraform {
  required_providers {
    kubernetes = {
      version = "~> 2.5.0"
      source  = "hashicorp/kubernetes"
    }
  }
}

provider "kubernetes" {
  config_path = "~/.kube/config"
}


resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  sha          = var.sha
  test_name    = "datadog_agent_remap_datadog_logs"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../../common/terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-toml = file("${path.module}/http_blackhole.toml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
}
module "http-gen" {
  source        = "../../../common/terraform/modules/lading_http_gen"
  type          = var.type
  http-gen-toml = file("${path.module}/http_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
}
```

This sets up a kubernetes provider pegged to minikube, creates a namespace
'soak' and installs three modules into that namespace: vector, http-blackhole
and http-gen. The module definitions are in the `common/` directory but suffice
to say they install vector and its lading test peers into 'soak', configuring
with the `toml` files referenced above. There are a handful of modules available
for use in soak testing; please add more as your infrastructure needs
dictate. If at all possible do not require services external to the minikube.

Newly added soaks in `tests/` will be ran automatically by CI.
