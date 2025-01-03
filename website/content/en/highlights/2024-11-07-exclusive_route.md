---
date: "2024-11-07"
title: "Exclusive Route Transform"
description: "Introducing the exclusive route transform"
authors: [ "pront" ]
pr_numbers: [ 21707 ]
release: "0.43.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: [ "transforms" ]
---

### Functionality

The Exclusive Route transform splits an event stream into unique sub-streams based on user-defined conditions. Each event will only be
routed to a single stream. This transforms complements the existing [Route transform][docs.transforms.route].

A visual representation:

<img src="/img/exclusive_route.svg" alt="Vector">

### Config Example

Let's see an example that demonstrates the above:

```yaml
# Sources section omitted

transforms:
  transform0:
    inputs:
      - source0
    type: exclusive_route
    routes:
      - name: "foo"
        condition: '.origin == "foo"'
      - name: "bar"
        condition: '.origin == "bar"'

# Sinks section omitted
```

[docs.transforms.route]: https://vector.dev/docs/reference/configuration/transforms/route/
