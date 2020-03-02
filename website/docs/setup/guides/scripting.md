---
title: Scripting Guide
sidebar_label: Scripting
description: Build custom transforms with Vector's scripting capabilities
status: beta
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import Alert from '@site/src/components/Alert';
import CodeHeader from '@site/src/components/CodeHeader';

When Vector's built-in [transforms][docs.reference.transforms] are not expressive enough for you needs,
it is possible to define your own transformation logic using scripted transforms.

Currently there are two supported scripted transforms, [`lua`][docs.reference.transforms.lua] and
[`javascript`][docs.reference.transforms.lua]. For most use cases they can be used interchangeably, so one
can chose between them based on personal preferences.

## Motivating Example

Let us take a look at a simple transform which enumerates incoming log events and then sends events with even
numbers to one lane and
events with odd numbers to another.

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>

<TabItem value="lua">

<CodeHeader fileName="splitter.lua" />

```lua
counter = 0

function handler (event)
  if not event.log then
    error("only log events are supported")
  end

  counter = counter + 1
  event.log.id = counter

  if counter % 2 == 0 then
    event.lane = "even"
  else
    event.lane = "odd"
  end

  return event
end
```

<CodeHeader fileName="vector.toml" />

```toml
[transforms.splitter]
  type = "lua"
  inputs = []
  path = "splitter.lua"
  handler = "handler"
```

</TabItem>

<TabItem value="javascript">

<CodeHeader fileName="splitter.js" />

```javascript
let counter = 0

function handler (event) {
  if (!event.log) {
    throw Error("only log events are supported")
  }

  counter++
  evnet.log.id = counter

  if (counter % 2 == 0) {
    event.lane = "even"
  } else {
    event.lane = "odd"
  }

  return event
}
```

<CodeHeader fileName="vector.toml" />

```toml
[transforms.splitter]
  type = "javascript"
  inputs = []
  path = "splitter.js"
  handler = "handler"
```

</TabItem>

</Tabs>

### Structure of the Script

The transform script from the example starts from setup code which is executed when the transform is created. It
initializes a global variable `counter` which value would persist in RAM throughout entire lifetime of the transform:

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>
<TabItem value="lua">

```lua
counter = 0
```

</TabItem>
<TabItem value="javascript">

```javascript
let counter = 0
```

</TabItem>
</Tabs>

Then it contains a definition of a handler function which processes events:

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>
<TabItem value="lua">

```lua
function handler (event)
  -- ...
end
```

</TabItem>
<TabItem value="javascript">

```javascript
function handler (event) {
  // ...
}
```

</TabItem>
</Tabs>

The handler function can have any name (thus, not necessarily `handler`). This name is specified as an option in the
definition of the transform in Vector's configuration file:

```toml
handler = "handler"
```

<Alert icon={false} type="info" classNames="list--infos">

* To save you a few keystrokes in common cases, Vector also supports a simplified configuration mode which uses
anonymous handler functions and allows to skip the `handler` option. See the section on
[anonymous handlers](#anonymous-handlers) for details.

</Alert>

When any of the inputs of the transform generates an event, the handler function is called with the event as its
argument.

### Representation of Events

Events are represented as tables in Lua and as objects in JavaScript. Vector uses
[externally tagged representation][urls.externally_tagged_representation] to encode both log and metric events in
a consistent fashion:

* [Log events][docs.about.data-model.log] are represented as values of a key named `log`.
* [Metric events][docs.about.data-model.metric] are represnted as values of a key named `metric`.

For instance, a typical log event produced by the [`stdin`][docs.reference.sources.stdin] source could have been
created programmatically using the following code:

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>
<TabItem value="lua">

```lua
event = {
  log = {
    host = "localhost",
    message = "the message",
    timestamp = os.date("!*t")
  }
}
```

</TabItem>
<TabItem value="javascript">

```javascript
event = {
  log: {
    host: "localhost",
    message: "the message",
    timestamp: new Date()
  }
}
```

</TabItem>
</Tabs>

A typical metric event could have been created programmatically in a similar way:

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>
<TabItem value="lua">

<Tabs
  defaultValue="gauge"
  select={true}
  urlKey="metric_kind"
  values={[
    { label: 'Counter', value: 'counter', },
    { label: 'Gauge', value: 'gauge', },
    { label: 'Set', value: 'set', },
    { label: 'Distribution', value: 'distribution', },
    { label: 'Aggregated Histogram', value: 'aggregated_histogram', },
    { label: 'Aggregated Summary', value: 'aggregated_summary', },
  ]
}>
<TabItem value="counter">

```lua
event = {
  metric = {
    name = "login.count",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host = "my.host.com"
    },
    value = {
      type: "counter",
      value: 24.2
    }
  }
}
```

</TabItem>
<TabItem value="gauge">

```lua
event = {
  metric = {
    name = "memory_rss",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host = "my.host.com"
    },
    value = {
      type = "gauge",
      value = 512.0
    }
  }
}
```

</TabItem>
<TabItem value="set">

```lua
event = {
  metric = {
    name = "user_names",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host = "my.host.com"
    },
    value = {
      type = "set",
      values = {"bob", "sam", "ben"}
    }
  }
}
```

</TabItem>
<TabItem value="distribution">

```javascript
event = {
  metric = {
    name = "response_time_ms",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host = "my.host.com"
    },
    value = {
      type = "distribution",
      values = {2.21, 5.46, 10.22},
      sample_rates = {5, 2, 5}
    }
  }
}
```

</TabItem>
<TabItem value="aggregated_histogram">

```lua
event = {
  metric = {
    name = "response_time_ms",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host = "my.host.com"
    },
    value = {
      type = "aggregated_histogram",
      buckets = {1.0, 2.0, 4.0, 8.0, 16.0, 32.0},
      counts = {20, 10, 45, 12, 18, 92},
      count = 197,
      sum = 975.2
    }
  }
}
```

</TabItem>
<TabItem value="aggregated_summary">

```lua
event = {
  metric = {
    name = "response_time_ms",
    timestamp = os.date("!*t"),
    kind = "absolute",
    tags = {
      host: "my.host.com"
    },
    value = {
      type = "aggregated_summary",
      quantiles = {0.1, 0.25, 0.5, 0.9, 0.99, 1.0},
      values = {2.0, 3.0, 5.0, 8.0, 9.0, 10.0},
      count = 197,
      sum = 975.2
    }
  }
}
```

</TabItem>
</Tabs>


</TabItem>
<TabItem value="javascript">

<Tabs
  defaultValue="gauge"
  select={true}
  urlKey="metric_kind"
  values={[
    { label: 'Counter', value: 'counter', },
    { label: 'Gauge', value: 'gauge', },
    { label: 'Set', value: 'set', },
    { label: 'Distribution', value: 'distribution', },
    { label: 'Aggregated Histogram', value: 'aggregated_histogram', },
    { label: 'Aggregated Summary', value: 'aggregated_summary', },
  ]
}>
<TabItem value="counter">

```javascript
event = {
  metric: {
    name: "login.count",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "counter",
      value: 24.2
    }
  }
}
```

</TabItem>
<TabItem value="gauge">

```javascript
event = {
  metric: {
    name: "memory_rss",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "gauge",
      value: 512.0
    }
  }
}
```

</TabItem>
<TabItem value="set">

```javascript
event = {
  metric: {
    name: "user_names",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "set",
      values: ["bob", "sam", "ben"]
    }
  }
}
```

</TabItem>
<TabItem value="distribution">

```javascript
event = {
  metric: {
    name: "response_time_ms",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "distribution",
      values: [2.21, 5.46, 10.22],
      sample_rates: [5n, 2n, 5n]
    }
  }
}
```

</TabItem>
<TabItem value="aggregated_histogram">

```javascript
event = {
  metric: {
    name: "response_time_ms",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "aggregated_histogram",
      buckets: [1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
      counts: [20n, 10n, 45n, 12n, 18n, 92n],
      count: 197n,
      sum: 975.2
    }
  }
}
```

</TabItem>
<TabItem value="aggregated_summary">

```javascript
event = {
  metric: {
    name: "response_time_ms",
    timestamp: new Date(),
    kind: "absolute",
    tags: {
      host: "my.host.com"
    },
    value: {
      type: "aggregated_summary",
      quantiles: [0.1, 0.25, 0.5, 0.9, 0.99, 1.0],
      values: [2.0, 3.0, 5.0, 8.0, 9.0, 10.0],
      count: 197n,
      sum: 975.2,
    }
  }
}
```

</TabItem>
</Tabs>

</TabItem>
</Tabs>

### Checks and Errors

Our handler function begins from checking whether the event it got as the argument a log event or not:

<Tabs
  block={true}
  defaultValue="lua"
  urlKey="lang"
  values={[
    { label: 'Lua', value: 'lua'},
    { label: 'JavaScript', value: 'javascript'},
  ]}>
<TabItem value="lua">

```lua
if not event.log then
  error("only log events are supported")
end
```

</TabItem>
<TabItem value="javascript">

```javascript
if (!event.log) {
  throw Error("only log events are supported")
}
```

</TabItem>
</Tabs>

If there is no `log` key in the event, the handler throws an exception which would be logged by Vector as an error. By
default Vector applies [rate limiting][docs.administration.monitoring#rate-limiting] to errors produced by the
transforms, so the errors messages would be easy to follow even for high event rate.

This check is not mandatory, but if you plan to share the code of the scripted transform between multiple
config files or users, having such a check simplifies diagnostics in case if the transform is applied to a wrong
kind of input.


[docs.about.data-model.log]: /docs/about/data-model/log/
[docs.about.data-model.metric]: /docs/about/data-model/metric/
[docs.administration.monitoring#rate-limiting]: /docs/administration/monitoring/#rate-limiting
[docs.reference.sources.stdin]: /docs/reference/sources/stdin/
[docs.reference.transforms.lua]: /docs/reference/transforms/lua/
[docs.reference.transforms]: /docs/reference/transforms/
[urls.externally_tagged_representation]: https://serde.rs/enum-representations.html#externally-tagged
