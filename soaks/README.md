# Soak Tests

This directory contains vector's soak tests, the integrated variant of our
benchmarks. The idea was first described in [RFC
6531](../rfcs/2021-02-23-6531-performance-testing.md) and has been steadily
improved as laid out in [Issue
9515](https://github.com/vectordotdev/vector/issues/9515).

## Requirements

In order to run a soak locally you will need:

* at least 6 CPUs
* at least 6Gb of RAM
* minikube
* [miller](https://github.com/johnkerl/miller)
* docker

The CPU and RAM requirements are currently hard-coded but might be made
flexible, possibly on a per-soak basis.

## Approach

The approach taken here is intentionally simplistic. A 'soak' is a defined set
of feature flags for building vector, a set of terraform to install vector and
support programs in a minikube and some glue code to observe vector in
operation. Consider this command:

```
> ./soaks/soak.sh datadog_agent_remap_datadog_logs a32c7fd09978f76a3f1bd360c3a8d07a49538b70 be8ceafbf994d06f505bdd9fb392b00e0ba661f2
```

Here we run the soak test `datadog_agent_remap_datadog_logs` comparing vector at
`a32c7fd09978f76a3f1bd360c3a8d07a49538b70` with vector at
`be8ceafbf994d06f505bdd9fb392b00e0ba661f2`. Two vector containers will be built
for each SHA. Time is saved by building vector only against the features needed
to complete the experiment. Users define these flags in files named `FEATURES`
under the soak directory, see
[`soaks/datadog_agent_remap_datadog_logs/FEATURES`]. The soak itself is defined
in terraform, see [`soaks/datadog_agent_remap_datadog_logs/terraform`].

After running this command you will, in about ten minutes depending on whether
you need to build containers or not, see a summary:

```
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
Higher numbers in the 'comparision' is better.

EXPERIMENT   SAMPLE_min       SAMPLE_p90       SAMPLE_p99       SAMPLE_max       SAMPLE_skewness  SAMPLE_kurtosis
baseline     24739333.118644  25918423.847458  26095157.813559  26160720.271186  -0.019132        -0.690881
comparision  35376407.491525  36809921.423729  36975943.016949  37141773.576271  -0.280115        -1.330509
```

The `baseline` experiment maps to the first SHA given to `soak.sh` --
`a32c7fd09978f76a3f1bd360c3a8d07a49538b70` -- and represents the starting point
of vector's throughput for this soak test. This baseline had a minimum observed
byte/second throughput of 24739333.118644/sec, a max of 26160720.271186/sec
etc. The comparision experiment has improved throughput -- higher numbers are
better -- even if the experiment was slightly more skewed than baseline and had
higher "tailedness". Improving this summary is a matter of importance.

## Defining Your Own Soak

This is premature.
