---
title: Vector Remap Language (VRL)
description: A domain-specific language for modifying your observability data
short: Vector Remap Language
weight: 1
aliases: ["/docs/reference/remap"]
---

Vector Remap Language (VRL) is an expression-oriented language designed for
transforming observability data (logs and metrics) in a [safe](#safety) and
[performant](#performance) manner. It features a simple [syntax](expressions)
and a rich set of built-in functions tailored specifically to observability use
cases.

You can use VRL in Vector via the [`remap`][remap] transform. For a more
in-depth picture, see the [announcement blog post][blog_post].

## Quickstart

VRL programs act on a single observability [event](#event) and can be used to:

- **Transform** observability events
- Specify **conditions** for [routing][route] and [filtering][filter] events

Those programs are specified as part of your Vector [configuration]. Here's an
example `remap` transform that contains a VRL program in the `source` field:

```toml {title="vector.toml"}
[transforms.modify]
type = "remap"
inputs = ["logs"]
source = '''
  del(.user_info)
  .timestamp = now()
'''
```

This program changes the contents of each event that passes through this
transform, [deleting][del] the `user_info` field and adding a [timestamp][now]
to the event.

### Example: parsing JSON

Let's have a look at a more complex example. Imagine that you're working with
HTTP log events that look like this:

```text
{
  "message": "{\"status\":200,\"timestamp\":\"2021-03-01T19:19:24.646170Z\",\"message\":\"SUCCESS\",\"username\":\"ub40fan4life\"}"
}
```

You want to apply these changes to each event:

- Parse the raw string into JSON
- Attempt to convert the timestamp and checks if the conversion was successful
- If the conversion is successful, convert the time to a UNIX timestamp; otherwise, use the current time
- Remove the `username` field
- Remove the temporary timestamp (`parsed_timestamp`) field
- Convert the `message` to lowercase

This VRL program would accomplish all of that:

```coffee
. = parse_json!(string!(.message))

.parsed_timestamp = parse_timestamp!(.timestamp, format: "%Y-%m-%dT%H:%M:%S.%fZ")

if is_timestamp(.parsed_timestamp) {
  .timestamp = to_unix_timestamp(.parsed_timestamp)
} else {
  .timestamp = to_unix_timestamp(now())
}

del(.username)
del(.parsed_timestamp)
.message = downcase(string!(.message))
```

Finally, the resulting event:

```json
{
  "message": "success",
  "status": 200,
  "timestamp": 1614626364
}
```

### Example: filtering events

The JSON parsing program in the example above modifies the contents of each
event. But you can also use VRL to specify conditions, which convert events into
a single Boolean expression. Here's an example [`filter`][filter] transform that
filters out all messages for which the `severity` field equals `"info"`:

```toml {title="vector.toml"}
[transforms.filter_out_info]
type = "filter"
inputs = ["logs"]
condition = '.severity != "info"'
```

Conditions can also be more multifaceted. This condition would filter out all
events for which the `severity` field is `"info"`, the `status_code` field is
greater than or equal to 400, and the `host` field isn't set:

```coffee
condition = '.severity != "info" && .status_code < 400 && exists(.host)
```

{{< info title="More VRL examples" >}} You can find more VRL examples further
down [on this page](#other-examples) or in the
[VRL example reference](/docs/reference/vrl/examples). {{< /info >}}

## Reference

All language constructs are contained in the following reference pages. Use
these references as you write your VRL programs:

{{< pages >}}

## Learn

VRL is designed to minimize the learning curve. These resources can help you get
acquainted with Vector and VRL:

{{< jump "/docs/setup/quickstart" >}} {{< jump "/guides/level-up/transformation"
>}}

{{< info title="VRL playground" >}} There is an online [VRL playground](https://playground.vrl.dev),
where you can experiment with VRL.

Some functions are currently unsupported on the playground. Functions that are currently not supported can be found with
this [issue filter](https://github.com/vectordotdev/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22vrl%3A+playground%22+wasm+compatible)
{{< /info >}}

## The goals of VRL {#goals}

VRL is built by the Vector team and its development is guided by two core goals,
[safety](#safety) and [performance](#performance), without compromising on
flexibility. This makes VRL ideal for critical, performance-sensitive
infrastructure, like observability pipelines. To illustrate how we achieve these,
below is a VRL feature matrix across these principles:

| Feature                                       | Safety | Performance |
| :-------------------------------------------- | :----- | :---------- |
| [Compilation](#compilation)                   | ✅      | ✅           |
| [Ergonomic safety](#ergonomic-safety)         | ✅      | ✅           |
| [Fail safety](#fail-safety)                   | ✅      |             |
| [Memory safety](#memory-safety)               | ✅      |             |
| [Vector and Rust native](#vector-rust-native) | ✅      | ✅           |
| [Statelessness](#stateless)                   | ✅      | ✅           |

## Concepts

VRL has some core concepts that you should be aware of as you dive in.

{{< vrl/concepts >}}

## Features

{{< vrl/features >}}

## Principles

{{< vrl/principles >}}

## Other examples

{{< vrl/real-world-examples >}}

[affine_types]: https://en.wikipedia.org/wiki/Substructural_type_system#Affine_type_systems
[blog_post]: /blog/vector-remap-language
[configuration]: /docs/reference/configuration
[dedupe]: /docs/reference/configuration/transforms/dedupe
[del]: /docs/reference/vrl/functions#del
[errors]: /docs/reference/vrl/errors
[events]: /docs/about/under-the-hood-architecture/data-model
[fail_safe]: https://en.wikipedia.org/wiki/Fail-safe
[ffi]: https://en.wikipedia.org/wiki/Foreign_function_interface
[filter]: /docs/reference/configuration/transforms/filter
[log]: /docs/reference/vrl/functions#log
[logs]: /docs/about/under-the-hood/architecture/data-model/log
[memory_safety]: https://en.wikipedia.org/wiki/Memory_safety
[metrics]: /docs/about/under-the-hood/architecture/data-model/metrics
[now]: /docs/reference/vrl/functions#now
[remap]: /docs/reference/configuration/transforms/remap
[route]: /docs/reference/configuration/transforms/route
[rust]: https://rust-lang.org
[rust_security]: https://thenewstack.io/microsoft-rust-is-the-industrys-best-chance-at-safe-systems-programming/
[vrl_error_handling]: /docs/reference/vrl/errors#handling
