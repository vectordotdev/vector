---
title: Exec
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Line delimiters

Each line is read until a new line delimiter, the `0xA` byte, is found *or* the end of the `maximum_buffer_size` is reached.
