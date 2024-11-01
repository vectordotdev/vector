---
title: Vector Remap Language
short: VRL
description: A fast and safe way to transform observability data
authors: ["binarylogic"]
date: "2021-02-15"
badges:
  type: announcement
  domains: ["remap"]
tags: ["vector remap language", "vrl", "dsl", "expression oriented"]
---

**Vector Remap Language** (VRL) is an [expression-oriented][expression_oriented]
language designed to work with observability data (logs and metrics) in a *safe*
and *performant* manner. It features a [simple syntax][vrl_expressions], a rich
set of [built-in functions][vrl_functions] tailored to observability use cases,
and [numerous features](#features) that set it far apart from other options.
This [0.12][0_12] release of Vector marks the official release of the language.

<!--more-->

VRL has been under intense but careful development for several months. We made
it available in beta beginning with the [0.11][0_11] release, but as of this
[0.12][0_12] release VRL is now generally available (GA). We're confident that
it's a *major* step forward for Vector and its users and the observability space
in general. Special thanks to [Jean Mertz][jean] and [Stephen Wakely][stephen]
for spearheading the VRL project.

*To jump right into the language, [skip to the solution section](#solution).
Otherwise, read on for why we created the language.*

## Preamble

I know what you may be thinking: another custom language? With so many out
there, some of them poorly done, challenging to learn, and just plain
unnecessary, you can be forgiven for being wary of another one on the pile. Rest
assured that we decided to create VRL only when it became clear that a new
language was the best way to improve upon our existing story for transforming
data.

But it's important to know at the outset that VRL is *not a programming
language* in the fullest sense. It lacks a wide range of programming constructs
that you find in all-purpose languages, such as loops, classes, modules, custom
functions, and IO access. This simplicity is *intentional*. We're confident that
Vector users will not chafe at the limitations of VRL and will in fact embrace
them as guarantors of sound design.

## The problem

Up to the 0.12 release, Vector has provided two types of transforms: **static
transforms** and **runtime transforms**.

* Static transforms do exactly *one* particular thing and are configuration
  based. The now-deprecated `remove_fields` transform, for example, would remove
  the fields that you specify in your Vector configuration.

* Runtime transforms enable you to modify event data using full-blown language
  runtimes, such as [Lua][lua].

Both of these options have enabled our users to transform their data
successfully, but they have severe limitations that we needed to address with
VRL:

1. Static transforms, while fast, are rigid, limited, and hard to manage. They
   leverage a configuration syntax for expressing data transformations. As a
   result, we've witnessed otherwise simple pipelines turn into hundreds of
   lines of configuration.

2. Runtime transforms are very robust, but users pay a steep performance and
   safety cost when using them. The robustness of using full-blown programming
   languages makes it very easy to write slow and error-prone programs that are
   difficult to collaborate on across a team.

The classic quadrant analysis looks something like this:

{{< svg "/img/blog/vrl-quadrant-comparison.svg" >}}

VRL eliminates this trade-off. Let's dig deeper on each of these points.

### Configuration languages are bad at expressing data transformations {#config-languages}

It's commonplace for observability pipelines to offer a rigid list of *static*
transforms that leverage a configuration syntax. Some call them "transforms",
others call them "filters" or "functions". If you zoom in, they make more sense
-- they perform a single task and make it difficult for users to do the wrong
thing. If you zoom out, they start to look like poorly designed programming
languages trying to be both a configuration *and* data transformation language.

{{< quote >}}
If you zoom out, they start to look like poorly designed programming languages,
trying to be both a configuration *and* data transformation language.
{{< /quote >}}

If you've worked with observability pipelines, it's not uncommon to have
hundreds of lines of configuration for relatively simple pipelines.

To demonstrate, let's transform a common-log (Apache) coming from Docker, a very
common use case:

```json
{
  "time": "2021-02-03T21:13:54.713161211Z",
  "stream": "stdout",
  "log": "5.86.210.12 - zieme4647 [03/Feb/2021:21:13:55 -0200] \"GET /embrace/supply-chains/dynamic/vertical HTTP/1.0\" 201 20574"
}
```

Parsing this log, without VRL, looks like this:

{{< tabs default="Vector" >}}
{{< tab title="Vector" >}}

```yaml title="vector.yaml"
# ... sources ...

transforms:
  parse_syslog:
    type: regex_parser
    inputs:
      - parse_docker
    patterns:
      - '^(?P<host>\S+) (?P<client>\S+) (?P<user>\S+) \[(?<timestamp>[\w:/]+\s[+\-]\d{4})\] "(?<method>\S+) (?<resource>.+?) (?<protocol>\S+)" (?<status>\d{3}) (?<bytes_out>\S+)$'
    field: log

  remove_log:
    type: remove_fields
    inputs:
      - parse_syslog
    fields:
      - time
      - log

  coerce_fields:
    type: coercer
    inputs:
      - remove_log
    types:
      timestamp: timestamp
      status: int
      bytes_out: int

# ... sinks ...
```

{{< /tab >}}
{{< tab title="Logstash" >}}

```text title="logstash.conf"
# ... inputs ...

filter {
  grok {
    match => { "message" => ["%{IPORHOST:[apache2][access][remote_ip]} - %{DATA:[apache2][access][user_name]} \[%{HTTPDATE:[apache2][access][time]}\] \"%{WORD:[apache2][access][method]} %{DATA:[apache2][access][url]} HTTP/%{NUMBER:[apache2][access][http_version]}\" %{NUMBER:[apache2][access][response_code]} %{NUMBER:[apache2][access][body_sent][bytes]}( \"%{DATA:[apache2][access][referrer]}\")?( \"%{DATA:[apache2][access][agent]}\")?",
      "%{IPORHOST:[apache2][access][remote_ip]} - %{DATA:[apache2][access][user_name]} \\[%{HTTPDATE:[apache2][access][time]}\\] \"-\" %{NUMBER:[apache2][access][response_code]} -" ] }
    remove_field => "message"
  }
  mutate {
    remove_field => [ "time", "log" ]
  }
  date {
    match => [ "[apache2][access][time]", "dd/MMM/YYYY:H:m:s Z" ]
    remove_field => "[apache2][access][time]"
  }
  mutate {
    coerce => {
      "[apache2][access][body_sent][bytes]" => "integer"
      "[apache2][access][response_code]" => "integer"
    }
  }
}

# ... outputs ...
```

{{< /tab >}}
{{< tab title="Fluentd" >}}

```text title="fluentd.conf"
<source>
  # ... source options ...
  format apache2
  tag apache.access
</source>

<parse>
  @type regexp
  expression /^(?<host>[^ ]*) [^ ]* (?<user>[^ ]*) \[(?<time>[^\]]*)\] "(?<method>\S+)(?: +(?<path>[^ ]*) +\S*)?" (?<code>[^ ]*) (?<size>[^ ]*)(?: "(?<referer>[^\"]*)" "(?<agent>[^\"]*)")?$/
  time_format %d/%b/%Y:%H:%M:%S %z
  types code:integer,size:integer
</parse>

<filter apache.access>
  @type record_transformer
  remove_keys time,log
</filter>

# ... outputs ...
```

{{< /tab >}}
{{< /tabs >}}


Which results in this general output:

```json
{
  "bytes_out": 20574,
  "host": "5.86.210.12",
  "method": "GET",
  "resource": "/embrace/supply-chains/dynamic/vertical",
  "protocol": "HTTP/1.0",
  "status": 201,
  "timestamp": "2021-02-03T23:13:55Z",
  "client": "-",
  "user": "zieme4647"
}
```

As you can see, configuration languages become verbose for even simple
pipelines, making them non-ideal for expressing data transformations. This is
because they have competing concerns: the better a language is at configuration,
the worse it is at data transformation.

{{< quote >}}
The better a language is at configuration, the worse it is at data
transformation.
{{< /quote >}}

Instead of conflating these concerns, like Logstash, Fluentd, and others, Vector
separates them, allowing you to choose your preferred configuration language
(YAML, TOML or JSON) while offering a purpose-built language for data
transformation (VRL). But is VRL really necessary? Couldn't you leverage Lua,
JavaScript, or any other existing language?

### Runtime transforms are slow and unsafe {#runtime-problems}

**Runtime** transforms enable you to modify event data using the full power of a
programming language runtime, such as Lua or JavaScript. They're robust enough
to handle even the thorniest use cases, but they have significant downsides that
make them high-risk for critical infrastructure like observability pipelines:

1. First, you pay a **significant performance penalty**. For example, on
   average, Lua is about 60% slower than Vector's Rust-based static transforms,
   and other runtimes like JavaScript are even slower.

2. Second, there are **severe security and safety risks** that make them show
   stoppers for critical infrastructure like observability pipelines. Things
   like the lack of memory-safety, dynamic code evaluation, access to IO,
   unaudited dependency trees, lack of sandboxing, and all of the security risks
   that come with that language runtime.

3. Finally, the extreme optionality presents **easy foot-guns** that manifest
   into real-world performance and reliability problems, making them difficult
   to manage.

Runtime transforms acted as robust escape hatches for Vector users, unblocking
exotic use cases and allowing us to learn more about the many ways people use
Vector. Still, we've seen the dark side of using them:

* Unexpected malformed data bringing pipelines down and waking operators up in
  the middle of the night.

* A poorly managed dependency tree introducing severe security risks,
  compromising their most sensitive data.

* Slow performance unable to keep up with data volume fluctuations.

* Complicated code leaving pipelines in an unmanageable state.

Because of these risks we recommend static transforms if given the choice, but
you shouldn't have to choose...

## Our solution: Vector Remap Language {#solution}

Before I dive into the specifics of VRL, let's look at the Docker, common-log
(Apache) example from above. If you skipped to this section we'll be parsing the
following log:

```json
{
  "time": "2021-02-03T21:13:54.713161211Z",
  "stream": "stdout",
  "log": "5.86.210.12 - zieme4647 [03/Feb/2021:21:13:55 -0200] \"GET /embrace/supply-chains/dynamic/vertical HTTP/1.0\" 201 20574"
}
```

Which can be achieved with the following VRL program:

```coffee
. = parse_common_log!(.log)
.total_bytes = del(.size)
.internal_request = ip_cidr_contains("5.86.0.0/16", .host) ?? false
```

Resulting in this output:

```json
{
  "host": "5.86.210.12",
  "internal_request": true,
  "user": "zieme4647",
  "timestamp": "2021-02-03T23:13:55Z",
  "message": "GET /embrace/supply-chains/dynamic/vertical HTTP/1.0",
  "method": "GET",
  "path": "/embrace/supply-chains/dynamic/vertical",
  "protocol": "HTTP/1.0",
  "total_bytes": 20574,
  "status": 201
}
```

As you can see, the above configuration is *significantly* easier to write,
read, and manage. Also, it maintains optimal performance and doesn't present the
[various security and safety](#runtime-problems) problems that come with runtime
transforms like Lua and JavaScript. You'll notice we threw in an extra field,
`internal_request`, to demonstrate how VRL solves otherwise difficult tasks for
static transforms.

The syntax was originally inspired by [jq] (note the `.` based paths), but
evolved into a simple multi-line language. For a deep dive into the language
constructs, check out the [reference documentation][vrl_reference]. Here you
will find a comprehensive list of [expressions][vrl_expressions],
[functions][vrl_functions], and [examples][vrl_examples].

### VRL principles and features {#features}

VRL was designed to exploit two principles: *safety* and *performance*, while
maintaining flexibility. This makes VRL ideal for always-up,
performance-sensitive infrastructure like observability pipelines. To illustrate
how we achieve this, below is a VRL feature matrix across these two principles:

| Feature                   | Safety | Performance |
|:--------------------------|:------:|:-----------:|
| [Progressive type safety] |   ✅    |             |
| [Fail safety]             |   ✅    |             |
| [Memory safety]           |   ✅    |             |
| [Ergonomic safety]        |   ✅    |      ✅      |
| [Vector/Rust native]      |   ✅    |      ✅      |
| [Stateless]               |   ✅    |      ✅      |

For more info on each click on the feature, but for the purposes of
demonstrating how VRL is unique, let's touch on the first two: *progressive type
safety* and *fail safety*.

### Progressive type and fail safety

A unique design decision behind VRL is its implementation of type and fail
safety. After seeing many users deal with pipeline instability due to runtime
errors, we made type and fail safety cornerstones. This makes VRL programs
**infallible**, ensuring that they will work as expected in production. Let's
demonstrate with an example, using the same Docker, common-log (Apache) example
from above.

Given this log:

```json
{
  "time":"2021-02-03T21:13:54.713161211Z",
  "stream": "stdout",
  "log": "5.86.210.12 - zieme4647 [03/Feb/2021:21:13:55 -0200] \"GET /embrace/supply-chains/dynamic/vertical HTTP/1.0\" 201 20574"
}
```

We want to parse it into this result:

```json
{
  "host": "5.86.210.12",
  "user": "zieme4647",
  "timestamp": "2021-02-03T23:13:55Z",
  "message": "GET /embrace/supply-chains/dynamic/vertical HTTP/1.0",
  "method": "GET",
  "path": "/embrace/supply-chains/dynamic/vertical",
  "protocol": "HTTP/1.0",
  "total_bytes": 20574,
  "status": 201
}
```

Someone new to VRL might write the following VRL program:

```coffee
. = parse_common_log(.log)
.total_bytes = del(.size)
```

And they'll be greeted with this thoughtful error message at compile-time
(Vector boot):

```rust
error[E103]: unhandled fallible assignment
  ┌─ :1:5
  │
1 │ . = parse_common_log(.log)
  │ --- ^^^^^^^^^^^^^^^^^^^^^^
  │ │   │
  │ │   this expression is fallible
  │ │   update the expression to be infallible
  │ or change this to an infallible assignment:
  │ ., err = parse_common_log(.log)
  │
  = see documentation about error handling at https://errors.vrl.dev/#handling
  = learn more about error code 103 at https://errors.vrl.dev/103
  = see language documentation at https://vrl.dev
```

As you can see, VRL requires the user to handle any expression that can result
in a runtime error. In our case, `.log` might not be a string, so we need to
either specify the type of the `.log` field or handle the error in the event
that `.log` is not a string. To resolve this error, the user must do one of
three things:

1.  **Handle the error**

    ```coffee
    ., err = parse_common_log(.log)
    if err != null {
      .malformed = true
      log("Failed to parse common-log: " + err, level: "error")
    } else {
      .total_bytes = del(.size)
    }
    ```

    While subtle, this has valuable implications! If the event is malformed
    (`.log` is not a formatted common-log string), then we forgo log an error
    and add a `.malformed` field. This preserves the original data and makes it
    easy to [route][route_transform] the malformed data for inspection. Not
    handling this error is a *very* common mistake that would otherwise result
    in data loss and downtime.

2.  **Raise the error and abort**

    ```coffee
    . = parse_common_log!(.log)
    .total_bytes = del(.size)
    ```

    In some cases, malformed data is unacceptable and the program must be
    aborted. In this case VRL offers *fallible function variants*. This is a
    fancy way of saying that suffixing your function call with `!` will raise an
    error upon failure and abort the program. Vector itself will continue on to
    process the next event, but will also log an error message to alert
    operators. Again, this forces the users to decide how to handle errors
    instead of being surprised by them.

3.  **Specify types**

    ```coffee
    .log = to_string!(.log)

    ., err = parse_common_log(.log)
    if err != null {
      # This error only occurs for malformed *strings*
      log("Failed to parse common-log: " + err, level: "error")
    } else {
      .total_bytes = del(.size)
    }
    ```

    Finally, a caveat of the two examples above is that it's hard to discern
    between a type error and a parsing error. Perhaps you want to abort on type
    errors and handle parsing errors? You can achieve this through *progressive
    type safety*. As a VRL program evaluates, it builds up type information
    about your fields. Once the type is known, subsequent usage of that field
    forgoes runtime type errors. Therefore, in the above example we know that
    the only possible way an error can occur is if parsing fails.

We're just scratching the surface on the language. For more information, check
out the VRL [reference][vrl_reference], [expressions][vrl_expressions],
[functions][vrl_functions], and [examples][vrl_examples].

## Should I start using VRL?

The short answer is an emphatic **yes**. For the use cases that VRL can
cover—and that should be most use cases—it should be seen as the preferred
option beginning with this release. If you aren't sure if your use case is
covered, don't ever hesitate to visit us [on Discord][chat].

## Coming soon

Although VRL should be considered ready for production use cases, there's more
on the way in the next few releases:

* The initial lineup of VRL functions can ably cover many use cases but we do
  have more functions in the pipeline.

* Expect major new features for simple object traversal.

* A web playground that enables you to experiment with VRL in the browser.

* Improved type safety and ergonomics through event schemas.

So stay tuned for much more work in this area. But for now, we strongly
encourage to explore the [VRL documentation][vrl_reference] and get in touch
with us [on Discord][chat] if you have any issues, comments, or suggestions.

[0_11]: /releases/0.11.0
[0_12]: /releases/0.12.0
[Affine type system]: /docs/reference/vrl/#affine-type-system
[chat]: https://chat.vector.dev
[Ergonomic safety]: /docs/reference/vrl/#ergonomic-safety
[expression_oriented]: https://en.wikipedia.org/wiki/Expression-oriented_programming_language
[Fail safety]: /docs/reference/vrl/#fail-safety
[jean]: https://github.com/JeanMertz
[jq]: https://stedolan.github.io/jq/
[lua]: https://vector.dev/docs/reference/transforms/lua/
[Memory safety]: /docs/reference/vrl/#memory-safety
[Progressive type safety]: /docs/reference/vrl/#progressive-type-safety
[route_transform]: /docs/reference/configuration/transforms/route/
[Stateless]: /docs/reference/vrl/#stateless
[stephen]: https://github.com/FungusHumungus
[Vector/Rust native]: /docs/reference/vrl/#vector-rust-native
[vrl_examples]: /docs/reference/vrl/examples/
[vrl_expressions]: /docs/reference/vrl/expressions/
[vrl_functions]: /docs/reference/vrl/functions/
[vrl_reference]: /docs/reference/vrl/
