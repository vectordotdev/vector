---
date: "2020-04-13"
title: "The Add Fields Transform Supports Templating"
description: "Use Vector's templating syntax to add new fields"
authors: ["binarylogic"]
pr_numbers: [1799]
release: "0.8.0"
hide_on_release_notes: true
badges:
  type: "enhancement"
  domains: ["transforms"]
  transforms: ["add_fields"]
---

Vector offers a [template syntax][docs.reference.templates] that you can use to build
dynamic values in your [Vector configuration][docs.setup.configuration] files. This
has now been added to the [`add_fields` transform][docs.transforms.remap],
enabling the ability to create fields from other fields values.

[docs.setup.configuration]: /docs/reference/configuration/
[docs.reference.templates]: /docs/reference/configuration/template-syntax
[docs.transforms.remap]: /docs/reference/configuration/transforms/remap/
