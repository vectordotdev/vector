---
title: Rollout
description: Strategies for rolling Vector out to production environments.
weight: 5
---

{{< warning >}}
This document assumes you’ve already decided on an architecture. If you have not, please read the [architecting document](/docs/setup/going-to-prod/architecting/).
{{< /warning >}}

## Rollout Strategy

Vector is designed to be deployed anywhere in your infrastructure, making it possible to follow the best practice of deploying within your network boundaries and avoiding a single point of failure. Our rollout strategy takes advantage of this through [incremental adoption](#incremental-adoption), [minimizing scope](#minimize-scope), allowing for [safe failure](#safe-failure), and [without the anxiety](#avoid-the-big-switch) of switching to a new system.

### Incremental Adoption

If you follow our [networking recommendations](/docs/setup/going-to-prod/architecting/#networking), then you should deploy Vector within each network partition (i.e., cluster or VPC), one at a time.

![Incremental Adoption](/img/going-to-prod/incremental-adoption.png)

This makes it easy to adopt Vector incrementally, allowing for sustainable progress while building out your observability pipeline.

### Minimize Scope

Minimizing scope is the easiest way to ensure success. The scope should be minimized to one network partition and one system at a time. Then, follow the [rollout plan](#rollout-plan) for each unit of scope.

### Safe Failure

While setting up Vector, it should be allowed to fail without consequence to your business. This means Vector should be operating on a redundant stream of data without disrupting your current production workflows.

![Safe Failure](/img/going-to-prod/safe-failure.png)

This allows you to gain confidence in Vector before your business depends on it.

### Avoid “The Big Switch”

Finally, by the time you cut over to Vector, you should have confidence in its ability. It should already be operating in a production capacity over a sustained period, removing any doubt that Vector will perform reliably in your production environment.

## Rollout Plan

{{< info >}}
Follow this plan for each deployment within each network partition.
{{< /info >}}

### 1. Deploy a Black Hole

- [Identify a single network partition](/docs/setup/going-to-prod/architecting/#boundaries) (i.e., cluster or VPC) for your Vector deployment.
- [Deploy Vector](/docs/setup/deployment/) with a single `blackhole` sink (the default) within that network partition.
- [Size and scale](/docs/setup/going-to-prod/sizing/) Vector’s instances for the [conservative estimate of 10 MiB/s/vCPU](/docs/setup/going-to-prod/sizing/#estimations).

### 2. Stream a Copy of Your Data

- Stream a copy of data from your agents to Vector. Verify that Vector receives data via the `vector top` and `vector tap` commands.

### 3. Configure Vector

- Process your data according to your use cases.
- Integrate Vector with your downstream systems. Verify data within each destination.

### 4. Size, Scale, & Soak

- Enable [autoscaling](/docs/setup/going-to-prod/sizing/#autoscaling) so that Vector can scale down appropriately.
- Soak Vector for at least 24 hours and monitor performance to ensure production readiness.

### 5. Cutover

- Safely shut down your agents to allow them to drain data without loss.
- Reconfigure your agents to send data to Vector exclusively.
- Start your agents.

---

{{< success >}}
Repeat for each network partition.
{{< /success >}}

## Support

For easy setup and maintenance of this architecture, consider Datadog Observability Pipelines, which comes with support.
