---
description: A "Hello World" getting started guide
---

# Sending Your First Event

This is a "Hello World" style guide that walks through sending your first
[event][docs.event] through Vector. It's designed to be followed locally,
making it quick and easy. We'll start with the simplest of examples: accepting
an event via the [`stdin` source][docs.sources.stdin], and then printing it out
via the [`console` sink][docs.sinks.console].

![][assets.getting-started-guide]

## 1. Install Vector

If you haven't already, [install Vector][docs.installation]:

```bash
curl https://sh.vector.dev -sSf | sh
```

Or view [platform specific installation instructions][docs.installation].

## 2. Send Your Event

Start by creating a temporary [Vector configuration file][docs.configuration]
in your home directory:

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

Voilà! The following is printed in your terminal:

```text
Starting Vector ...
172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272
```

Exit Vector by pressing `ctrl+c`.

Notice that Vector prints the same raw line that you sent it. This is because
Vector does not awkwardly enforce structuring on you until you need it, which
brings us to parsing...

## 3. Parse Your Event

In most cases you'll want to parse your event into a structured format. Vector
makes this easy with [transforms][docs.transforms]. In this case, we'll use
the [`regex_parser`][docs.transforms.regex_parser]. Let's update your existing
Vector configuration file:

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

Voilà! The following is printed in your terminal:

```text
Starting Vector ...
{"host": "172.128.80.109", "message": 
```

Exit `vector` by pressing `ctrl+c`.

You'll notice this time the event is structured. Vector knows when an event
is structured or not and defaults to JSON encoding for outputs that support
it. You can change the encoding in the
[`console` sink options][docs.sinks.console].

That's it! This tutorial demonstrates the _very_ basic [concepts][docs.concepts]
of Vector. From here, you can start to think about the various
[sources][docs.sources], [transforms][docs.transforms], and [sinks][docs.sinks]
you'll need to combine to create your pipelines.


[assets.getting-started-guide]: ../../assets/getting-started-guide.svg
[docs.concepts]: ../../about/concepts.md
[docs.configuration]: ../../usage/configuration
[docs.event]: ../../setup/getting-started/sending-your-first-event.md
[docs.installation]: ../../setup/installation
[docs.sinks.console]: ../../usage/configuration/sinks/console.md
[docs.sinks]: ../../usage/configuration/sinks
[docs.sources.stdin]: ../../usage/configuration/sources/stdin.md
[docs.sources]: ../../usage/configuration/sources
[docs.transforms.regex_parser]: ../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../usage/configuration/transforms
