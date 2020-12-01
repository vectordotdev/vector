---
last_modified_on: "2020-04-13"
$schema: ".schema.json"
title: "The Add Fields Transform Supports Templating"
description: "Use Vector's templating syntax to add new fields"
author_github: "https://github.com/binarylogic"
pr_numbers: [1799]
release: "0.8.0"
hide_on_release_notes: true
tags: ["type: enhancement", "domain: transforms", "transform: add_fields"]
---

Vector offers a [template syntax][docs.reference.templates] that you can use to build
dynamic values in your [Vector configuration][docs.setup.configuration] files. This
has now been added to the [`add_fields` transform][docs.transforms.add_fields],
enabling the ability to create fields from other fields values.

[docs.setup.configuration]: /docs/setup/configuration/
[docs.reference.templates]: /docs/reference/templates/
[docs.transforms.add_fields]: /docs/reference/transforms/add_fields/
