# Target Specification

This document specifies requirements for platforms ("targets") for the
integration of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

Other words, such as "agent", "aggregator", "node", and "service" are to be
interpreted as described in the [terminology document][terminology_document].

- [Introduction](#introduction)
- [Deployment Architectures](#deployment-architectures)
  - [Agent Architecture](#agent-architecture)
  - [Aggregator Architecture](#aggregator-architecture)
  - [Unified Architecture](#unified-architecture)
- [Hardening](#hardening)

## Introduction

In its simplest form, installing Vector consists of downloading the binary and
making it executable, but leaves much to be desired for users looking to
integrate Vector in real-world production environments. To adhere with Vector's
["reduce decisions" design principle], Vector must also be opinionated about how
it's deployed, providing easy facilities for adopting Vector's
[reference architectures][reference_architectures],
[achieving high availability][high_availability], and [hardening][hardening]
Vector.

## Deployment Architectures

When supporting a target, Vector must support them through the the paradigm of
architectures:

* Targets MUST support the [agent architecture][agent_architecture] by
  providing a single command that deploys Vector and achieves the
  [agent architecture requirements](#agent-architecture).
* Targets MUST support the [aggregator architecture][aggregator_architecture] by
  providing a single command that deploys Vector and achieves the
  [aggregator architecture requirements](#aggregator-architecture).
* Targets MUST support the [unified architecture][unified_architecture] by
  providing a single command that deploys Vector and achieves the
  [unified architecture requirements](#aggregator-architecture).

### Agent Architecture

The [agent architecture][agent_architecture] deploys Vector on each individual
node for local data collection. The following requirements define suppoprt for
this architecture:

* Architecture
  * MUST deploy as a daemon on existing nodes, one Vector process per node.
  * MUST NOT deploy Vector aggregator nodes, since the Vector aggregator can be
    configured to assume agent responsibilities.
  * MUST deploy with Vector's [default agent configuration][default_agent_configuration]
    which largely covers the agent architecture
    [design recommendations][agent_architecture_design].
* Sizing
  * MUST deploy as a good infrastructure citizen, giving resource priority to
    other services on the same node.
  * SHOULD be limited to 2 vCPUs, MUST be overridable by the user.
  * SHOULD be limited to 2 GiB of memory per vCPU (4 GiB in this case), MUST be
    overridable by the user.
  * SHOULD be limited to 1 GiB of disk space, MUST be overridable by the user.

### Aggregator Architecture

* Architecture
  * MUST deploy as a stateful service on dedicated nodes, Vector is the only
    service on the node.
  * MUST deploy with a persistent disk that is available between deployments.
  * MUST deploy with Vector's [default aggregator configuration][default_aggregator_configuration]
    which largely covers the aggregator architecture
    [design recommendations][aggregator_architecture_design].
    in order to achieve durability with disk buffers and source checkpoints.
  * SHOULD deploy within one Cluster or VPC at a time.
  * Configured Vector ports, including non-default user configured ports,
    SHOULD be automatically accessible within the Cluster or VPC.
  * Configured Vector sources, including non-default user configured sources,
    SHOULD be automatically discoverable via target service discovery
    mechanisms.
* High Availability
  * SHOULD deploy across 2 nodes, MUST be overridable by the user.
  * SHOULD deploy across 2 availability zones, MUST be overridable by the user.
* Sizing
  * MUST deploy in a way that takes full advantage of all system resources.
  * SHOULD request 8 vCPUs, MUST be overridable by the user.
  * SHOULD request 2 GiB of memory per vCPU (16 GiB in this case), MUST be
    overridable by the user.
  * SHOULD be limited to 1 GiB of disk space, MUST be overridable by the user.

### Unified Architecture

TODO: Should we support this as a top-level architecture, or have users deploy
both the agent and aggregator separately and integrate them by default.

## Hardening

TODO



[agent_architecture]: https://www.notion.so/Agent-Architecture-3e3c9950398f4f349dff9e83ac6dea83
[agent_architecture_design]: ...
[default_agent_configuration]: ...
[default_aggregator_configuration]: ...
[hardening]: https://www.notion.so/Hardening-1fcce789ceaa4ea1ac9bc39112ee7224
[high_availability]: https://www.notion.so/High-Availability-b2a44d37e88a4bae83677139b3979872
[reference_architectures]: https://www.notion.so/08e711506fd446be947ce0674dfc370e?v=43ee3d19efbb4b34b55593ce9761e9bc
[terminology_document]: ...