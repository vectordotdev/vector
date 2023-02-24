# Target Specification

This document specifies requirements for installation targets for the
integration of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

Other words, such as "agent", "aggregator", "node", and "service" are to be
interpreted as described in the [terminology document][terminology_document].

- [1. Introduction](#1-introduction)
- [2. Installation Targets](#2-installation-targets)
- [3. Deployment Architectures](#3-deployment-architectures)
  - [4. Agent Architecture](#4-agent-architecture)
  - [5. Aggregator Architecture](#5-aggregator-architecture)
  - [6. Unified Architecture](#6-unified-architecture)
- [7. Hardening](#7-hardening)

## 1. Introduction

In its simplest form, installing Vector consists of downloading the binary and
making it executable, but leaves much to be desired for users looking to
integrate Vector in real-world production environments. To adhere with Vector's
["reduce decisions" design principle][reduce_decisions], Vector must also be opinionated about how
it's deployed, providing easy facilities for adopting Vector's
[reference architectures][reference_architectures],
[achieving high availability][high_availability], and [hardening][hardening]
Vector.

## 2. Installation Targets

Vector supports a number of installation targets that can be categorized into:

- Virtual/Physical Machine
- Orchestration Platform

The primary differentiator between the two being that Virtual/Physical Machines
provide a single node as the deployment target, whereas Orchestration Platforms
allow for a scheduler to deploy Vector across a number of nodes. These categories
have their own requirements for each
[Deployment Architecture](#3-deployment-architectures).

Examples of Virtual/Physical Machine targets include, but are not limited to:

- Debian
- Docker
- RHEL
- Windows

Examples of Orchestration Platform targets include, but are not limited to:

- Kubernetes

## 3. Deployment Architectures

When supporting a target, Vector must support them through the paradigm of
architectures:

- Targets MUST support the [agent architecture][agent_architecture] by
  providing a single command that deploys Vector and achieves the
  [agent architecture requirements](#agent-architecture).
- Targets SHOULD support the [aggregator architecture][aggregator_architecture] by
  providing a single command that deploys Vector and achieves the
  [aggregator architecture requirements](#aggregator-architecture).
- Targets MAY support the [unified architecture][unified_architecture] by
  providing a single command that deploys Vector and achieves the
  [unified architecture requirements](#unified-architecture).

### 4. Agent Architecture

The [agent architecture][agent_architecture] deploys Vector on each individual
node for distributed data collection and processing. Along with general
[hardening](#7-hardening) requirements, the following requirements define support
for this architecture:

- Architecture
  - MUST deploy as a daemon on existing nodes, one Vector process per node.
  - MUST deploy with Vector's [default agent configuration][default_agent_configuration].
- Sizing
  - MUST deploy as a good infrastructure citizen, giving resource priority to
    other services on the same node.
  - SHOULD be limited to 1 vCPUs by default, MUST be overridable by the user.
  - SHOULD be limited to 2 GiB of memory per vCPU by default, MUST be
    overridable by the user.
  - SHOULD be limited to 1 GiB of disk space, MUST be overridable by the user.

### 5. Aggregator Architecture

The [aggregator architecture][aggregator_architecture] deploys Vector onto
dedicated nodes for data aggregation. Along with general [hardening](#7-hardening)
requirements, the following requirements define support for this architecture:

- Architecture
  - MUST deploy as a service with reserved/dedicated resources.
  - SHOULD deploy with a persistent disk that is available between deployments by default,
    MUST be overridable by the user if they do not want a persistent disk.
  - MUST deploy with Vector's [default aggregator configuration][default_aggregator_configuration].
  - Configured Vector ports, including non-default user configured ports,
    SHOULD be automatically accessible within the Cluster or VPC.
  - Configured Vector sources, including non-default user configured sources,
    SHOULD be automatically discoverable via target service discovery
    mechanisms.
- Sizing
  - MUST have dedicated/reserved resources that cannot be stolen by other services, preventing
    the "noisy neighbor" problem to the degree possible.
  - The Vector service SHOULD NOT be artificially limited with resource
    limiters such as cgroups.
  - SHOULD require 8 vCPUs by default, MUST be overridable by the user.
  - SHOULD require 2 GiB of memory per vCPU (16 GiB in this case) by default,
    MUST be overridable by the user.
  - SHOULD request 36 GiB of disk space per vCPU by default (288 GiB in this case),
    MUST be overridable by the user.

The following are additional requirements for Orchestration Platform installation
targets:

- High Availability
  - SHOULD deploy across multiple nodes by default, MUST be overridable by the user.
  - SHOULD deploy across multiple availability zones by default, MUST be overridable by the user.
- Scaling
  - SHOULD provide facilities for provisioning a load balancer to enable horizontal scaling
    out of the box. MUST be overridable by the user.
    - Cloud-managed load balancers (i.e., AWS NLB) SHOULD be supported in addition to
      self-managed load balancers (i.e., HAProxy).
    - Cloud-managed load balancers SHOULD be prioritized by default over self-managed
      load balancers.
    - Network load balancers (layer-4) SHOULD be prioritized over HTTP load balancers (layer-7)
  - Autoscaling SHOULD be enabled by default, driven by an average of 85%
    CPU utilization and a stabilization period of 5 minutes.

### 6. Unified Architecture

The [unified architecture][unified_architecture] deploys Vector on each
individual node as an agent and as a separate service as an aggregator.
The requirements for both the [agent](#4-agent-architecture) and the
[aggregator](#5-aggregator-architecture) apply to this architecture.
This architecture SHOULD NOT be installed on Virtual/Physical Machine
targets as there is little added benefit.

## 7. Hardening

- Setup
  - An unprivileged Vector service account SHOULD be created upon installation
    for running the Vector process.
- Data hardening
  - Swap SHOULD be disabled to prevent in-flight data from leaking to disk.
    Swap would also make Vector prohibitively slow.
  - Vector's data directory SHOULD be read and write restricted to Vector's
    dedicated service account.
  - Core dumps SHOULD be prevented for the Vector process to prevent in flight
    data from leaking to disk.
- Process hardening
  - Vector's artifacts
    - All communication during the setup process, such as downloading Vector
      artifacts, MUST use encrypted channels.
    - Downloaded Vector artifacts MUST be verified against the provided
      checksum.
    - The latest Vector version SHOULD be downloaded unless otherwise specified
      by the user.
  - Vector's configuration
    - Vector's configuration directory SHOULD be read restricted to Vector's
      service account.
  - Vector's runtime
    - Vector SHOULD be run under an unprivileged, dedicated service account.
    - Vector's service account SHOULD NOT have the ability to overwrite Vector's
      binary or configuration files. The only directory the Vector service
      account should write to is Vector’s data directory.
- Network hardening
  - Configured sources and sinks SHOULD use encrypted channels by default.

[agent_architecture]: https://vector.dev/docs/setup/going-to-prod/arch/agent/
[aggregator_architecture]: https://vector.dev/docs/setup/going-to-prod/arch/aggregator/
[default_agent_configuration]: https://github.com/vectordotdev/vector/blob/master/config/agent/vector.yaml
[default_aggregator_configuration]: https://github.com/vectordotdev/vector/blob/master/config/aggregator/vector.yaml
[hardening]: https://vector.dev/docs/setup/going-to-prod/hardening/
[high_availability]: https://vector.dev/docs/setup/going-to-prod/high-availability/
[reduce_decisions]: https://github.com/vectordotdev/vector/blob/master/docs/USER_EXPERIENCE_DESIGN.md#be-opinionated--reduce-decisions
[reference_architectures]: https://vector.dev/docs/setup/going-to-prod/arch/
[rfc 2119]: https://datatracker.ietf.org/doc/html/rfc2119
[terminology_document]: https://vector.dev/docs/reference/glossary/
[unified_architecture]: https://vector.dev/docs/setup/going-to-prod/arch/unified/
