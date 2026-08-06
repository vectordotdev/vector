# RFC 7469 - 2021-05-16 - Scaling and load balancing for Vector aggregators in Kubernetes

This RFC describes the need for a tooling and environment agnostic load balancing solution to be bundled with the Vector aggregator Helm chart.

* [Scope](#scope)
* [Out of Scope](#out-of-scope)
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

Load balancing will be a concern for any `sink` or `source` supported by Vector; some of which can use a general solution (ex: load balancing for our HTTP based `sinks`) and some of which are specific to the component (ex: Kafka `source`/`sink`). Due to the breadth of the topic, this RFC will focus on three specific cases for load balancing to Vector aggregators in Kubernetes, while also giving consideration to future adoption for other components.

* Vector agent to Vector aggregator
* Datadog agent to Vector aggregator
* Syslog (TCP) agent to Vector aggregator

## Out of Scope

* Scaling the Kafka `source`
* Scaling and load balancing on platforms other than Kubernetes

## Motivation

Today, scaling Vector horizontally (by increasing replicas) is a manual process when deployed as an aggregator. This limits Vector aggregators in both reliability and performance, causing adoption concerns for users. A single aggregator will be limited in performance by the resources that can be dedicated to it, presumably with some (currently) unknown upper bounds. Vector aims to be vendor neutral, and as such we should provide the capacity to scale and load balance across Vector aggregators regardless of environment or upstream event collectors.

## Internal Proposal

Include a configuration for a dedicated reverse proxy that will be deployed as part of the vector-aggregator Helm chart. We should provide basic, but functional, configurations out-of-the box to enable users to "one click" install Vector as an aggregator. The proxy should dynamically resolve downstream Vector instances and allow users to update the balance config to provide for more consistent targets in situations that require it (aggregation transforms). I propose our initially supported proxy should be HAProxy, with the next second being NGINX or Envoy. HAProxy, compared to NGINX, provides more metrics (exposed as JSON or in Prometheus format) and has native service discovery to dynamically populate its configuration. Lua can be used with NGINX to provide service discovery, for example the [nginx-ingress-controller](https://kubernetes.github.io/ingress-nginx/).

HAProxy intentionally has little support for proxying UDP, as of 2.3 there is support for forwarding syslog traffic however it doesn't allow for dynamic backend configuration greatly limiting the usability for us.

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
* A dedicated reverse proxy will be specialized and optimized for its task, and the same can be said for Vector itself.

## Prior Art

* [Logstash: Scaling TCP, UDP, and HTTP](https://www.elastic.co/guide/en/logstash/current/deploying-and-scaling.html#_tcp_udp_and_http_protocols)
* [Fluentd: Aggregator behind Network Load Balancer](https://aws.amazon.com/blogs/compute/building-a-scalable-log-solution-aggregator-with-aws-fargate-fluentd-and-amazon-kinesis-data-firehose/)

## Drawbacks

* The team will need to maintain a configuration for a third-party application, as well as ensuring the application is kept up-to-date and free of any reported vulnerabilities.
* We will also need to add the reverse proxy to new or existing integration tests to ensure there are no regressions with our provided configuration and proxy version.
* Our deployment will be more complex and require an additional application for end users. This can create more misdirection while debugging and additional operational burden.
* HAProxy has limited support for proxying UDP, and thus the initial implementation won't support load balancing for UDP `sources`.
* HAProxy's forwarding for syslog doesn't allow for dynamic backend servers, because of this syslog over UDP isn't going to be supported by the initial implementation.

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

* [x] Which reverse proxy to use? HAProxy, NGINX, Envoy, Traefik, etc. It should be widely used, battle-tested, support most/all protocols Vector uses, and preferably well understood by multiple members of our team.
* [x] Should built-in load balancing capabilities be explored (where possible)? Internal load balancing options would simplify operations for end users who are all-in on Vector. - This is probably more appropriate on a different RFC, or per component.
* [x] Do we always need to ensure requests are made to the same downstream aggregator, or only a specific subset of requests? - Default balancing will be `roundrobin`, with documentations around setting to `source` as an alternative
* [x] Each `source` needs its unique port; what defaults and/or templating do we provide to the load balancer? - Out of the box configurations for Datadog agents and Vector agents
* [x] How will users monitor the load balancer? Logs, metrics, and health. - We will provide out of the box configurations for Datadog agents and Vector to collect and process the proxy's logs and metrics, allowing the user to route them with the rest of their data.

## Plan Of Attack

* [ ] Manually confirm functionality for Vector, Datadog agents, and syslog over the proxy
* [ ] Include the (optional) proxy deployment in the vector-aggregator chart, add to e2e kubernetes test suite, add docs
* [ ] Include out-of-the box configuration for Datadog agents to load balance across Vector aggregators, add to e2e kubernetes test suite, add docs
* [ ] Provide out-of-the box configuration to collect and process the proxy observability data for Datadog agents and Vector, add docs
