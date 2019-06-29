---
description: A "Hello World" style guide
---

# Sending Your First Event

{% hint style="info" %}
This guide assumes you've already [installed](../installation/) Vector. If you have not, please install Vector before proceeding.
{% endhint %}

This is a "Hello World" style guide that walks through sending your first [event](../../about/data-model.md#event) through Vector. It designed to be followed locally, making it quick and easy. We'll start with the simplest of examples: accepting an event via the [`stdin` source](../../usage/configuration/sources/stdin.md), and then printing it out via the [`console` sink](../../usage/configuration/sinks/console.md).

![](../../assets/getting-started-guide.svg)

## 1. Send Your Event

Start by creating a temporary [Vector configuration file](../../usage/configuration/) in your home directory:

{% code-tabs %}
{% code-tabs-item title="~/vector.toml" %}
```bash
echo '
[sources.in]
    type = "stdin"

[sinks.out]
    inputs = ["in"]
    type = "console"
' > ~/vector.toml
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Now pipe an event through Vector:

```bash
echo '172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272' | vector --config ~/vector.toml
```

Viola! The following is printed in your terminal:

```text
Starting Vector ...
172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272
```

Exit `vector` by pressing `ctrl+c`.

Notice that Vector prints the same raw line that you sent it. This is because Vector does not awkwardly enforce structuring on you until you need it, which brings us to parsing...

## 2. Parse Your Event

In most cases you'll want to parse your event into a structured format. Vector makes this easy with [transforms](../../usage/configuration/transforms/). In this case, we'll use the [`regex_parser`](../../usage/configuration/transforms/regex_parser.md). Let's update your existing Vector configuration file:

```bash
echo '
[sources.in]
    type = "stdin"

# Structure and parse the data
[transforms.apache_parser]
    inputs = ["in"]
    type   = "regex_parser"
  regex    = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

[sinks.out]
    inputs = ["apache_parser"]
    type = "console"
' > ~/vector.toml
```

Let's pipe the same event again through Vector:

```bash
echo '172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272' | vector --config ~/vector.toml
```

Viola! The following is printed in your terminal:

```text
Starting Vector ...
{"host": "172.128.80.109", "message": 
```

Exit `vector` by pressing `ctrl+c`.

You'll notice this time the event is structured. Vector knows when an event is structured or not and defaults to JSON encoding for outputs that support it. You can change the encoding in the [`console` sink options](../../usage/configuration/sinks/console.md).

That's it! This tutorial demonstrates the _very_ basic [concepts](../../about/concepts.md) of Vector. From here, you can start to think about the various [sources](../../usage/configuration/sources/), [transforms](../../usage/configuration/transforms/), and [sinks](../../usage/configuration/sinks/) you'll need to combine to create your pipelines.



