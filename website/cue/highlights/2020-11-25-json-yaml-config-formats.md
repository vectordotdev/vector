---
last_modified_on: "2020-11-25"
$schema: ".schema.json"
title: "JSON & YAML config formats are now supported"
description: "We've added support for JSON and YAML config formats."
author_github: "https://github.com/binarylogic"
pr_numbers: [4856, 5144]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: config"]
---

To ensure Vector fits into existing workflows, like Kubernetes, we've added
support for JSON and YAML config formats in addition to TOML. TOML will
continue to be our default format, but users are welcome to use JSON and YAML
as they see fit.

## Use cases

### Kubernetes

YAML is the preferred language in the K8s ecosystem, and to ensure Vector
does not feel awkward in the Kubernetes platform we now support YAML for
our pipeline definitions.

### Data templating languages

A bonus to supporting JSON is the enablement of data templating languages like
[Jsonnet][jsonnet] and [Cue][cue]. For example, a Vector user has already
built an [unofficial Jsonnet library][jsonnet_library] for Vector.

## Get Started

Use Vector as you normally work, but pass it `yaml` or `json` config files:

```bash
vector --config /etc/vector/vector.json
```

Vector will infer the format from the extension. This is especially useful
when passing multiple config files via globs:

```bash
vector --config /etc/vector/*
```

Additionally, we've provided format specific flags for edge cases:

```bash
vector --config-json /etc/vector/vector.json
```

Head over to the [configuration docs][config] for more info.

[config]: /docs/setup/configuration/
[cue]: https://cuelang.org/
[jsonnet]: https://jsonnet.org/
[jsonnet_library]: https://github.com/xunleii/vector_jsonnet
