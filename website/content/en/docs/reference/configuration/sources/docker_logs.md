---
title: Docker logs
description: Collect logs from [Docker](https://docker.com)
kind: source
---

## Warnings

{{< component/warnings >}}

## Setup

The `docker_logs` source is part of a larger setup strategy for the Docker platform.

{{< jump "/docs/setup/installation/platforms/docker" >}}

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Context

{{< snippet "context" >}}

### Merging split messages

By default, Docker splits log messages that exceed 16k in size. This can be a rather frustrating problem because it produces malformed log messages that are difficult to work with. Vector solves this by automatically merging these messages into a single message. You can disable this via the [`auto_partial_merge`](#auto_partial_merge) option. Furthermore, you can adjust the marker that Vector uses to determine if an event is partial via the `partial_event_marker_field` option.

### State

{{< snippet "stateless" >}}
