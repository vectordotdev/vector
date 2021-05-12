---
title: Vector Remap Language (VRL)
description: A domain-specific language for modifying your observability data
short: Remap Language
weight: 4
---

Vector Remap Language (VRL) is an expression-oriented language designed for transforming observability data (logs and metrics) in a [safe](#safety) and [performant](#performance) manner. It features a simple [syntax](expressions) and a rich set of built-in functions tailored specifically to observability use cases.

You can use VRL in Vector via the [`remap`][remap] transform. For a more in-depth picture, see the [announcement blog post][blog_post].

## Quickstart

VRL programs act on a single observability event and can be used to:

* **Transform** observability events
* Specify **conditions** for [routing][route] and [filtering][filter] events

Those programs are specified as part of your Vector [configuration]. Here's an example `remap` transform that contains a VRL program in the `source` field:

```toml {title="vector.toml"}
[transforms.modify]
type = "remap"
inputs = ["logs"]
source = '''
  del(.user_info)
  .timestamp = now()
'''
```

This program changes the contents of each event that passes through this transform, [deleting][del] the `user_info` field and adding a [timestamp][now] to the event.

### Example: parsing JSON

Let's have a look at a more complex example. Imagine that you're working with HTTP log events that look like this:

```
"{\"status\":200,\"timestamp\":\"2021-03-01T19:19:24.646170Z\",\"message\":\"SUCCESS\",\"username\":\"ub40fan4life\"}"
```

You want to apply these changes to each event:

* Parse the raw string into JSON
* Reformat the `time` into a UNIX timestamp
* Remove the `username` field
* Convert the `message` to lowercase

This VRL program would accomplish all of that:

```ruby
. = parse_json!(string!(.message))
.timestamp = to_unix_timestamp(to_timestamp!(.timestamp))
del(.username)
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

The JSON parsing program in the example above modifies the contents of each event. But you can also use VRL to specify conditions, which convert events into a single Boolean expression. Here's an example [`filter`][filter] transform that filters out all messages for which the `severity` field equals `"info"`:

```toml {title="vector.toml"}
[transforms.filter_out_info]
type = "filter"
inputs = ["logs"]
condition = '.severity != "info"'
```

Conditions can also be more multifaceted. This condition would filter out all events for which the `severity` field is `"info"`, the `status_code` field is greater than or equal to 400, and the `host` field isn't set:

```vrl
condition = '.severity != "info" && .status_code < 400 && exists(.host)
```

{{< info title="More VRL examples" >}}
You can find more VRL examples further down [on this page](#other-examples) or in the [VRL example reference](/docs/reference/vrl/examples).
{{< /info >}}

## Reference

All language constructs are contained in the following reference pages. Use these references as you write your VRL programs:

{{< pages >}}

## Learn

VRL is designed to minimize the learning curve. These resources can help you get acquainted with Vector and VRL:

{{< jump "/docs/setup/quickstart" >}}
{{< jump "/guides/level-up/transformation" >}}

## Concepts

{{< vrl/concepts >}}

VRL is built by the Vector team and its development is guided by two core goals, [safety](#safety) and [performance](#performance), without compromising on flexibility. This makes VRL ideal for critical, performance-sensitive infrastructure, like observabiity pipelines. To illustrate how we achieve these, below is a VRL feature matrix across these principles:

Feature | Safety | Performance
:-------|:-------|:-----------
[Compilation](#compilation) | ✅ | ✅
[Ergonomic safety](#ergonomic-safety) | ✅ | ✅
[Fail safety](#fail-safety) | ✅ |
[Memory safety](#memory-safety) | ✅ |
[Vector and Rust native](#vector-rust-native) | ✅ | ✅
[Statelessness](#statelessness) | ✅ | ✅

### Features

#### Compilation

VRL programs are compiled to and run as native [Rust] code. This has several important implications:

* VRL programs are extremely fast and efficient, with performance characteristics very close to Rust itself
* VRL has no runtime and thus imposes no per-event foreign function interface (FFI) or data conversion costs
* VRL has no garbage collection, which means no GC pauses and no accumulated memory usage across events

#### Fail safety checks

At compile time, Vector performs [fail safety](#fail-safety) checks to ensure that all errors thrown by fallible functions are [handled][vrl_error_handling]. If you fail to pass a string to the `parse_syslog` function, for example, the VRL compiler aborts and provides a helpful error message. Fail safety means that you need to make explicit decisions about how to handle potentially malformed data—a superior alternative to being surprised by such issues when Vector is already handling your data in production.

#### Type safety checks

At compile time, Vector performs [type safety](#type-safety) checks to catch runtime errors stemming from type mismatches, for example passing an integer to the `parse_syslog` function, which can only take a string. VRL essentially forces you to write programs around the assumption that every incoming event could be malformed, which provides a strong bulwark against both human error and also the many potential consequences of malformed data.

#### Ergonomic safety

VRL is ergonomically safe in that it makes it difficult to create slow or buggy VRL programs. While VRL's [compile-time checks](#compilation) prevent runtime errors, they can't prevent some of the more elusive performance and maintainability problems that stem from program complexity—problems that can result in observability pipeline instability and unexpected resource costs. To protect against these more subtle ergonomic problems, VRL is a carefully limited language that offers only those features necessary to transform observability data. Any features that are extraneous to that task or likely to result in degraded ergonomics are omitted from the language by design.

* **Internal logging limitation**

  VRL programs do produce internal logs but not at a rate that's bound to saturate I/O.

* **I/O limitation**

  VRL lacks access to system I/O, which tends to be computationally expensive, to require careful caching, and to produce degraded performance.

* **Lack of recursion**

  VRL lacks recursion capabilities, making it impossible to create large or infinite loops that could stall VRL programs or needlessly drain memory.

* **Lack of custom functions**

  VRL requires you to use only its built-in functions and doesn't enable you to create your own. This keeps VRL programs easy to debug and reason about.

* **Lack of state**

  VRL doesn't maintain state across events. This prevents things like unbounded memory growth, hard-to-debug production issues, and unexpected program behavior.

* **Rate-limited logging**

  The VRL [`log`][log] function implements rate limiting by default. This ensures that VRL programs invoking the log method don't accidentally saturate I/O.

* **Purpose built for observability**

  VRL is laser focused on observability use cases and only those use cases. This makes many frustration- and complexity-producing constructs you find in other languages completely superfluous. Functions like `parse_syslog` and `parse_key_value`, for example, make otherwise complex tasks simple and prevent the need for complex low-level constructs.

#### Fail safety

VRL programs are [fail safe][fail_safe], meaning that a VRL program doesn't compile unless all errors thrown by fallible functions are handled. This eliminates unexpected runtime errors that often plague production observability pipelines with data loss and downtime. See the [error reference][errors] for more information on VRL errors.

#### Logs and metrics

VRL works with both [logs] and [metrics] within Vector, making it usable for all [Vector events][events].

#### Memory safety

VRL inherits Rusts's [memory safety][memory_safety] guarantees, protecting you from [common software bugs and security vulnerabilities][rust_safety] that stem from improper memory access. This makes VRL ideal for infrastructure use cases, like observability pipelines, where reliability and security are top concerns

#### Vector and Rust native

Like Vector, VRL is built with [Rust] and compiles to native Rust code. Therefore, it inherits Rust's safety and performance characteristics that make it ideal for observability pipelines. And because both VRL and Vector are written in Rust, they are tightly integrated, avoiding communication inefficiencies such as event serialization or [foreign function interfaces][ffi] (FFI). This makes VRL significantly faster than non-Rust alternatives.

* **Lack of garbage collection**

  Rust's [affine type system][affine_types] avoids the need for garbage collection, making VRL exceptionally fast, memory efficient, and memory safe. Memory is precisely allocated and freed, avoiding the pauses and performance pitfalls associated with garbage collectors.

#### Clear error messages

VRL strives to provide high-quality, helpful error messages, streamling the development and iteration workflow around VRL programs. This VRL program, for example...

```ruby
.foo, err = upcase(.foo)
```

...would result in this error:

```ruby
error: program aborted
  ┌─ :2:1
  │
2 │ parse_json!(1)
  │ ^^^^^^^^^^^^^^
  │ │
  │ function call error
  │ unable to parse json: key must be a string at line 1 column 3
  │
  = see function documentation at: https://master.vector.dev/docs/reference/vrl/functions/#parse_json
  = see language documentation at: /docs/reference/vrl
```

#### Statelessness

VRL programs are stateless, operating on a single event at a time. This makes VRL programs simple, fast, and safe. Operations involving state across events, such as [deduplication][dedupe], are delegated to other Vector transforms designed specifically for stateful operations.

#### Type safety

VRL implements *progressive* [type safety](#type-safety), erroring at [compile time](#compilation) if a type mismatch is detected.

* **Progressive type safety**

  VRL's type safety is *progressive*, meaning that it implements type safety for any value for which it knows the type. Because observability data can be quite unpredictable, it's not always known which type a field might be, hence the progressive nature of VRL's type-safety. As VRL scripts are evaluated, type information is built up and used at compile-time to enforce type-safety. Let's look at an example:

  ```ruby
  .foo # any
  .foo = downcase!(.foo) # string
  .foo = upcase(.foo) # string
  ```

  Breaking down the above:

  1. The `foo` field starts off as an `any` type (aka unknown).
  1. The call to the `downcase!` function requires error handling (`!`) since VRL can't guarantee that `.foo` is a string (the only type supported as an input to `downcase`).
  1. Afterwards, assuming the `downcase` invocation is successful, VRL knows that `.foo` is a string, since `downcase` can only return strings.
  1. Finally, the call to `upcase` doesn't require error handling (`!`) since VRL knows that `.foo` is a string, making the `upcase` invocation infallible.

  To avoid error handling for argument errors, you can specify the types of your fields at the top of your VRL script:

  ```ruby
  .foo = string!(.foo) # string

  .foo = downcase(.foo) # string
  ```

  This is a good practice in general, and it enables you to opt into type safety as you see fit. VRL scripts are written once and evaluated many times, thus the trade-off for type safety ensures reliable execution in production.

## Principles

### Safety

VRL is a safe language in several senses: VRL scripts have access only to the event data that they handle and not, for example, to the Internet or the host; VRL provides the same strong memory safety guarantees as Rust; and, as mentioned above, compile-time correctness checks prevent VRL scripts from behaving in unexpected or sub-optimal ways. These factors distinguish VRL from other available event data transformation languages and runtimes.

### Performance

VRL is implemented in the very fast and efficient [Rust] language and VRL scripts are compiled into Rust code when Vector is started. This means that you can use VRL to transform observability data with a minimal per-event performance penalty vis-à-vis pure Rust. In addition, ergonomic features such as compile-time correctness checks and the lack of language constructs like loops make it difficult to write scripts that are slow or buggy or require optimization.

## Other examples

{{< vrl/real-world-examples >}}

## Pages in this section

{{< pages >}}

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
