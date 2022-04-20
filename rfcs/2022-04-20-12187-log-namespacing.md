# RFC 12187 - 2022-04-20 - Log Namesapcing

Today, when deserializing data on an event the keys are arbitrary and can potentially collide with data on the root.
Not only is an inconvenient loss of data, but it prevents us from fully utilizing the power of schemas.
The event data should be restructured to prevent collisions, and in general make them easier to use.

## Context

- Link to any previous issues, RFCs, or briefs (do not repeat that context in this RFC).

## Cross cutting concerns

- Proof of concept with the datadog agent logs sink [12218](https://github.com/vectordotdev/vector/pull/12218)

## Scope

### In scope

- TBD

### Out of scope

- Expanding schema support to event metadata


## Proposal

### User Experience

- Explain your change as if you were describing it to a Vector user. We should be able to share this section with a Vector user to solicit feedback.
- Does this change break backward compatibility? If so, what should users do to upgrade?


### Examples

#### Datadog Agent source / JSON codec / Vector namespace
event
```json
{
  "data": {
    "derivative": -2.266778047142367e+125,
    "integral": "13028769352377685187",
    "mineral": "H 9 ",
    "proportional": 3673342615,
    "vegetable": -30083
  },
  "ddsource": "waters",
  "ddtags": "env:prod",
  "hostname": "beta",
  "service": "cernan",
  "status": "notice",
  "timestamp": "2066-08-09T04:24:42.1234Z" // This is parsed from a unix timestamp provided by the DD agent
}
```
metadata
```json
{
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

#### Datadog Agent source / bytes codec / Vector namespace
event
```json
{
  "data": "{\"proportional\":702036423,\"integral\":15089925750456892008,\"derivative\":-6.4676193438086e263,\"vegetable\":20003,\"mineral\":\"vsd5fwYBv\"}",
  "ddsource": "waters",
  "ddtags": "env:prod",
  "hostname": "beta",
  "service": "cernan",
  "status": "notice",
  "timestamp": "2066-08-09T04:24:42.1234Z"
}
```
metadata
```json
{
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```


#### Kafka source / json codec / Vector namespace
event
```json
{
  "data": {
    "derivative": -2.266778047142367e+125,
    "integral": "13028769352377685187",
    "mineral": "H 9 ",
    "proportional": 3673342615,
    "vegetable": -30083
  },
  "key": "the key of the message"
  // headers were originally nested under a configurable "headers_key". This is using a static value.
  "headers": {
    "header-a-key": "header-a-value",
    "header-b-key": "header-b-value",
  }
}
```
metadata
```json
{
  "topic": "name of topic",
  "partition": 3,
  "offset": 1829448,
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```


#### Kubernetes Logs / Vector namespace

event
```text
// yes, this is a string at the root.
F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
```

metadata
```json
{
  "file": "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log",
  "container_image": "gcr.io/k8s-minikube/storage-provisioner:v3",
  "container_name": "storage-provisioner",
  "namespace_labels": {
    "kubernetes.io/metadata.name": "kube-system"
  },
  "pod_annotations": {
    "prometheus.io/scrape": "false"
  },
  "pod_ip": "192.168.1.1",
  "pod_ips": [
    "192.168.1.1",
    "::1"
  ],
  "pod_labels": {
    "addonmanager.kubernetes.io/mode": "Reconcile",
    "gcp-auth-skip-secret": "true",
    "integration-test": "storage-provisioner"
  },
  "pod_name": "storage-provisioner",
  "pod_namespace": "kube-system",
  "pod_node_name": "minikube",
  "pod_uid": "93bde4d0-9731-4785-a80e-cd27ba8ad7c2",
  "source_type": "kubernetes_logs",
  "stream": "stderr",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```



### Implementation

- Explain your change as if you were presenting it to the Vector team.
- When possible, demonstrate with pseudo code not text.
- Be specific. Be opinionated. Avoid ambiguity.

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives



## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
