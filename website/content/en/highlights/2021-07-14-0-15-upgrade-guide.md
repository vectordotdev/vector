---
date: "2021-07-14"
title: "0.15 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.15.0"
authors: ["jszwedko"]
pr_numbers: []
release: "0.15.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.15.0 release includes a few minor breaking changes:

1. [Dropping support for Kubernetes 1.14.X.](#first)
2. [The `sample` transform now takes VRL conditions by default.](#second)
3. [The `remap` condition type was renamed to `vrl`.](#third)

We cover each below to help you upgrade quickly:

## Upgrade Guide

### Dropping support for Kubernetes 1.14 {#first}

We've dropped support for Kubernetes version 1.14 as it is no longer a supported Kubernetes version nor do major cloud
providers support it.

This version of Kubernetes is likely to still work with Vector, but we will no longer be testing against it and cannot
guarantee future versions of Vector will be compatible.

In general, we will aim to match [Kubernetes' supported releases][kubernetes_releases] and along with the versions
supported by major cloud providers.

### The `sample` transform now takes VRL conditions by default {#second}

In version 0.12.0 we switched to using [VRL] as the default condition type for our transforms that take conditions (like
`filter` and `route`), but the `sample` transform was not updated even though its documentation was. This release brings
the `sample` transform in line with the others by making [VRL] the default condition type.

This means, if you are presently using the deprecated `check_fields` syntax, you will need to add `type
= "check_fields"` to your condition.

For example, if you previously had:

```toml
[transforms.sample]
type = "sample"
inputs = ["in"]
rate = 10
key_field = "message"
exclude."message.contains" = "error"
```

You will need to add `exclude.type = "check_fields"` like:

```toml
[transforms.sample]
type = "sample"
inputs = ["in"]
rate = 10
key_field = "message"
exclude."type" = "check_fields"
exclude."message.contains" = "error"
```

To convert this to the new [VRL][VRL] conditions, you would write:

```toml
[transforms.sample]
type = "sample"
inputs = ["in"]
rate = 10
key_field = "message"
exclude = """
  contains!(.message, "error")
"""
```

We recommend upgrading to the [VRL][VRL] conditions as these are much more powerful than the legacy `check_fields`-style
conditions.

### The `remap` condition type was renamed to `vrl` {#third}

The `remap` condition type has been renamed `vrl` in this release to better highlight that the syntax for it is
a [VRL][VRL] program. Most examples of using this condition type have the short-hand condition config of just specifying
the [VRL][VRL] program without specifying a `type`. For example:

```toml
[transforms.filter_a]
  inputs = ["stdin"]
  type = "filter"
  condition = '''
      message = if exists(.tags) { .tags.message } else { .message }
      message == "test filter 1"
    '''
```

Which is automatically a [VRL][VRL] condition. However, if you were specifying the `type` like:

```toml
[transforms.filter_a]
inputs = ["stdin"]
type = "filter"
condition.type = "remap"
condition.source = '''
    message = if exists(.tags) { .tags.message } else { .message }
    message == "test filter 1"
'''
```

Then you will need to update `type = "remap"` to `type = "vrl"` like:

```toml
[transforms.filter_a]
inputs = ["stdin"]
type = "filter"
condition.type = "vrl"
condition.source = '''
    message = if exists(.tags) { .tags.message } else { .message }
    message == "test filter 1"
'''
```

[vrl]: /docs/reference/vrl/
[kubernetes_releases]: https://kubernetes.io/releases/
