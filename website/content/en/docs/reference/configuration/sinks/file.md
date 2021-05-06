---
title: File
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### File and directory creation

Vector attempts to create the entire directory structure and the file when emitting events to the `file` sink. This requires that the Vector agent has the correct permissions to create and write to files in the specified directories.

### Health checks

{{< snippet "health-checks" >}}

### State

{{< snippet "stateless" >}}
