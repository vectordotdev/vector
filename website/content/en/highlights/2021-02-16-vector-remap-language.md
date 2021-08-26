---
date: "2021-02-16"
title: "Introducing Vector Remap Language"
description: "A lean, fast, and safe language for transforming observability data."
authors: ["binarylogic"]
featured: true
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "featured"
  domains: ["remap"]
---

The Vector team is excited to announce the **Vector Remap Language** (VRL) is an expression-oriented language designed to work with observability data (logs and metrics) in a *safe* and *performant* manner. It features a [simple syntax][vrl_expressions], a rich set of [built-in functions][vrl_functions] tailored to observability use cases, and [numerous features][vrl_features] that set it far apart from other options. This 0.12 release of Vector marks the  official release of the language.

Read the announcement post:

{{< jump "/blog/vector-remap-language" >}}

## Further reading

If your interest in VRL is now piqued, we recommend checking out these resources:

* The [VRL announcement post][post] on the Vector blog
* The [VRL documentation][vrl_reference]
* VRL [examples][vrl_examples]

[docs]: /docs/reference/vrl/
[examples]: /docs/reference/vrl/examples/
[expression_oriented]: https://en.wikipedia.org/wiki/Expression-oriented_programming_language
[jq]: https://stedolan.github.io/jq
[post]: /blog/vector-remap-language/
[vrl_examples]: /docs/reference/vrl/examples/
[vrl_expressions]: /docs/reference/vrl/expressions/
[vrl_features]: /docs/reference/vrl/#features
[vrl_functions]: /docs/reference/vrl/functions/
[vrl_reference]: /docs/reference/vrl/
