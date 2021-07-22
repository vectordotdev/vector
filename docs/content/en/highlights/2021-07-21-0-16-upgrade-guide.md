---
date: "2021-07-21"
title: "0.16 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.16.0"
authors: ["jszwedko"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.16.0 release includes one breaking change:

1. [Component name field renamed to ID](#first)

We cover it below to help you upgrade quickly:

[##](##) Upgrade Guide

### Component name field renamed to ID {#first}

Historically we've referred to the component ID field as `name` in some places, `id` in others. We've decided to
standardize on `ID` as we feel this is more closer to the intention of the field: an unchanging identifier for
components.

For example, with the component config:

```toml
[transforms.parse_nginx]
type = "remap"
inputs = []
source = ""
```

The `parse_nginx` part of the config is now only referred to as `ID`.

This required a couple of breaking changes to Vector's internal metrics:

* For metrics coming from `internal_metrics`, the `component_name` tag has been updated to be `component_id`. If you
  were grouping by this tag in your metrics queries, or referring to it in a `remap` or `lua` transform, you should
  update it to refer to `component_id`.
* Within the GraphQL API, all references to `name` for `Component`s has been updated to be `componentId`. This is used
  over simply `Id` as `Id` has special semantics within the GraphQL ecosystem and we may add support for this field
  later.
