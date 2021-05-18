# RFC 7469 - 2021-05-16 - Scaling and load balancing for Vector aggregators

This RFC describes the need for a tooling and environment agnostic load balancing solution to be bundled with Vector aggregator deployments.

* [Scope](#scope)
* [Motivation](#motivation)
* [Internal Proposal](#internal-proposal)
* [Rationale](#rationale)
* [Prior Art](#prior-art)
* [Drawbacks](#drawbacks)
* [Alternatives](#alternatives)
  * [Do Nothing](#do-nothing)
  * [Only Client-side Load Balancing](#only-client-side-load-balancing)
  * [Require a Service Mesh](#require-a-service-mesh)
* [Outstanding Questions](#outstanding-questions)
* [Plan Of Attack](#plan-of-attack)

## Scope

Load balancing will be a concern for any `sink` or `source` supported by Vector; some of which can use a general solution (ex: load balancing for our HTTP based `sinks`) and some of which are specific to the component (ex: Kafka or Elasticsearch `sinks`). Due to the breadth of the topic, this RFC will focus on three specific cases for load balancing while also giving consideration to future adoption for other components.

* Vector agent to Vector aggregator
* Datadog agent to Vector aggregator
* Syslog agent to Vector aggregator

## Motivation

Today Vector lacks the capacity to scale horizontally (by increasing replicas) when deployed as an aggregator. This limits Vector aggregators in both reliability and performance, causing adoption concerns for users. A single aggregator will be limited in performance by the resources that can be dedicated to it, presumably with some (currently) unknown upper bounds. Vector aims to be vendor neutral, and as such we should provide the capacity to scale and load balance across Vector aggregators regardless of environment or upstream event collectors.

## Internal Proposal

Include a configuration for a dedicated reverse proxy that will be deployed as part of the vector-aggregator Helm chart, as well as documented configuration and installation instructions for users that run outside of Kubernetes. We should provide basic, but functional, configurations out-of-the box to enable users to "one click" install Vector as an aggregator. The proxy should dynamically resolve downsteam Vector instances but allow for consistent targets in situations that require it (aggregation transforms).

Below is a basic HAProxy configuration configured to leverage service discovery in a Kubernetes cluster:

```haproxy
resolvers coredns
    nameserver dns1 kube-dns.kube-system.svc.cluster.local:53
    hold timeout         600s
    hold refused         600s 
frontend vector
    bind *:9000
    default_backend vector_template
backend vector_template
    balance roundrobin
    option tcp-check         
    server-template srv 10 _vector._tcp.vector-aggregator-headless.vector.svc.cluster.local resolvers coredns check
```

## Rationale

* Configuring an external reverse proxy for load balancing allows for load balancing regardless of the upstream agent.
* Using a dedicated reverse proxy to load balance requests for Vector aggregators should support the largest spread of `sources` with the smallest amount of engineering effort.
* A solution outside of Vector itself ensures that users can reliably adopt Vector as an aggregator without replacing their existing infrastructure.
* Most organizations are likely familiar with operating _some_ class of reverse proxy.
* Dedicated reverse proxy can be specialized and optimized for its task, and the same can be said for Vector itself.

## Prior Art

* [Logstash: Scaling TCP, UDP, and HTTP](https://www.elastic.co/guide/en/logstash/current/deploying-and-scaling.html#_tcp_udp_and_http_protocols)
* [Fluentd: Aggregator behind Network Load Balancer](https://aws.amazon.com/blogs/compute/building-a-scalable-log-solution-aggregator-with-aws-fargate-fluentd-and-amazon-kinesis-data-firehose/)
* [Cribl: Bring your own load balancer](https://docs.cribl.io/docs/deploy-distributed#architecture)

## Drawbacks

* The team will need to maintain a configuration for a third-party application, as well as ensuring the application is kept up-to-date and free of any reported vulnerabilities.
* We will also need to add the reverse proxy to new or existing integration tests to ensure there are no regressions with our provided configuration and proxy version.
* Our deployment will be more complex and require an additional application for end users. This can create more misdirection while debugging and additional operational burden.

## Alternatives

### Do Nothing

The Vector aggregator can currently function as a single instance and be scaled vertically rather than horizontally. While this reduces complexity, it causes Vector to be a single point of failure and introduces an upper limit for throughput.

### Only Client-side Load Balancing

The library powering the v2 Vector `sink`/`source` does provide the capabilities to do client-side load balancing, however that just covers a single `sink` to `source` pairing. For certain clients like Beats and Logstash we could implement an Elasticsearch compatible API and allow those clients to use their native load balancing and integrations, this would generally be per source and not available for all sources.

### Require a Service Mesh

Users already leveraging a service mesh could offload the load balancing to the mesh, however, requiring a service mesh to run and scale Vector aggregators horizontally is a large barrier to adoption.

### Distributed hashring

Project like Thanos and Loki have used hashrings to enable multi-tenancy, we could likely do something similar to ensure events are forwarded to the correct aggregator. I don't think anyone wants to turn Vector into a distributed system though.

## Outstanding Questions

* [ ] Which reverse proxy to use? HAProxy, NGINX, Envoy, Traefik, etc. It should be widely used, battle-tested, support most/all protocols Vector uses, and preferably well understood by mutliple members of our team.
* [ ] Should built-in load balancing capabilities be explored (where possible)? Internal load balancing options would simplify operations for end users who are all-in on Vector.
* [ ] Do we always need to ensure requests are made to the same downstream aggregator, or only a specific subset of requests?
* [ ] Is a generic reverse proxy "context aware" enough to ensure data is always routed as required?
* [ ] Each `source` needs its unique port; what defaults and/or templating do we provide to the load balancer?

## Plan Of Attack

* [ ] ...

Note: This can be filled out during the review process.
