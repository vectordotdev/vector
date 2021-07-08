---
date: "2020-10-27"
title: "New metrics integrations"
description: "Collect metrics from your host, Apache, Nginx, and Mongodb."
authors: ["jamtur01"]
pr_numbers: [3704, 4157, 4500, 4698, 5209]
release: "0.11.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["metrics", "sources"]
---

Vector 0.11 includes new metrics sources:

1. **A new [`host_metrics` source][host_metrics_source]**
2. **A new [`apache_metrics` source][apache_metrics_source]**
3. **A new [`nginx_metrics` source][nginx_metrics_source]**
4. **A new [`mongodb_metrics` source][mongodb_metrics_source]**
5. **A new [`aws_ecs_metrics` source][aws_ecs_metrics_source]**
6. **A new [`internal_metrics` source][internal_metrics_source]**

And while these are only six sources, they represent a broader initiative
to replace metrics agents entirely. A lot of groundwork was laid to expedite
these types of integrations, so you can expect many more of them in
subsequent Vector releases.

## Agent fatigue, we're coming for you

For anyone that manages observability pipelines, it's not uncommon to deploy
multiple agents on a single host (an understatement). We've seen setups
that deploy five or more agents on a single host -- using more than _30% of the
CPU resources for that host_! We cover this in detail in our
[Kubernetes announcements post][kubernetes_announcement]. It's a genuine and
costly problem. Vector has its sights set on solving this. We want Vector to be
the single agent for all of your logs, metrics, and traces.

## Get Started

To get started with these sources, define them and go:

```toml
[sources.host_metrics]
type = "host_metrics" # or apache_metrics, mongodb_metrics, or internal_metrics

# Then connect them to a sink:
[sinks.prometheus]
type = "prometheus"
inputs = ["host_metrics"]
```

Tada! One agent for all of your data. Check out the [docs][docs] for more
details.

## Switching from another metrics agent?

We'd love to chat! We're eager to unblock the transition. If Vector is missing
a metrics integration or feature, [chat with us][chat]. We are working closely
with a number of organizations to assist with this transition.

[apache_metrics_source]: /docs/reference/configuration/sources/apache_metrics/
[aws_ecs_metrics_source]: /docs/reference/configuration/sources/aws_ecs_metrics/
[chat]: https://chat.vector.dev
[docs]: /docs
[host_metrics_source]: /docs/reference/configuration/sources/host_metrics/
[internal_metrics_source]: /docs/reference/configuration/sources/internal_metrics/
[kubernetes_announcement]: /blog/kubernetes-integration/
[mongodb_metrics_source]: /docs/reference/configuration/sources/mongodb_metrics/
[nginx_metrics_source]: /docs/reference/configuration/sources/nginx_metrics/
