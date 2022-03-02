---
title: Custom Aggregations with Lua
description: Write a custom transform for aggregating log events into metrics using Lua scripting
authors: ["binarylogic"]
domain: transforms
transforms: ["lua"]
weight: 1
tags: ["lua", "aggregation", "transform", "advanced", "guides", "guide"]
---

While Vector's built-in [transforms][docs.transforms] are fast, sometimes they are not expressive
enough for your needs. In such cases the [`lua`][docs.transforms.lua] transform comes to rescue, letting you
define custom transformation logic.

This guide walks through various ways of defining an aggregating transform component that takes incoming log events,
counts them, and emits [`counter`][docs.architecture.data-model.metric#counter] metrics each 5 seconds.

## Architectural Overview

Lua is an interpreted language embedded into Vector. When a `lua` transform is created, it starts an instance
of the Lua interpreter. As a consequence, different transforms are isolated and cannot interrupt each other.

The execution model is asynchronous, with two key concepts: _hooks_ and _timer handlers_. Both of them are
user-defined Lua functions which are called by Vector at certain events.

### Hooks

There are three types of hooks: `init`, `process`, and `shutdown`.

#### The `process` hook

The most important of them is `process`, which is called on each incoming events. It can be defined like this:

```toml
hooks.process = """
function (event, emit)
  -- do something
end
"""
```

It takes a single event and can output one or many of them using the `emit` function provided as the second argument.

For example, the body of the function above could have been

```lua
event.log.my_field = "my value"
emit(event)
```

The code above sets field `my_field` to value `"my_value"` and sends the newly created event to the downstream
components. Read more about event representation [in the reference][docs.transforms.lua#event-data-model].

#### The `init` hook

The `init` hook is similar to `process` hook, but it is called before the first call of the `process` hook, and thus
takes no events as its arguments.

Note that it although it is called before the first event, it is called only after the first event is ready to be
processed. However, one should not rely on this behavior, as it is not guaranteed to not change in the future.

#### The `shutdown` hook

The `shutdown` hook is called after the last event is received. It doesn't take events as arguments as well.

### Timer handlers

Timer handlers are similar to hooks by being Lua functions capable of producing events. However, they are called
periodically at pre-defined intervals.


All of the functions listed above share a single runtime, so they can communicate between each other using global
variables.

## First Implementation

Using the knowledge from the previous section, it is possible to write down the following transform definition:

```toml title="vector.toml"
[transforms.aggregator]
type = "lua"
version = "2"
inputs = [] # add IDs of the input components here

hooks.init = """
  function (emit)
    count = 0 -- initialize state by setting a global variable
  end
"""

hooks.process = """
  function (event, emit)
    count = count + 1 -- increment the counter and exit
  end
"""

timers = [{interval_seconds = 5, handler = """
  function (emit)
    emit {
      metric = {
        name = "event_counter",
        kind = "incremental",
        timestamp = os.date("!*t"),
        counter = {
          value = counter
        }
      }
    }
    counter = 0
  end
"""}]

hooks.shutdown = """
  function (emit)
    emit {
      metric = {
        name = "event_counter",
        kind = "incremental",
        timestamp = os.date("!*t"),
        counter = {
          value = counter
        }
      }
    }
  end
"""
```

One could plug it into a [pipeline][docs.about.concepts#pipelines] and it would work!

However, this code could and should be refactored. Hold on to the next section to see how it could be done.

## Reduce Duplication

A bird's-eye view of the transform definition reveals that the timer handler and the shutdown hook are almost
identical. It is possible make the config more [DRY][urls.dry_code] by extracting creation of the counter into
a dedicated function. Such a function can be placed into the [source][docs.transforms.lua#source]
section of the config:

```toml
source = """
  function make_counter(value)
    return metric = {
      name = "event_counter",
      kind = "incremental",
      timestamp = os.date("!*t"),
      counter = {
        value = value
      }
    }
  end
"""
```

and then adjusting the timer handler

```toml
timers = [{interval_seconds = 5, handler = """
  function (emit)
    emit(make_counter(counter))
    counter = 0
  end
"""}]
```

and the `shutdown` hook:

```toml
hooks.shutdown = """
  function (emit)
    emit(make_counter(counter))
  end
"""
```

## Keep All Code Together

The new config looks tidier, but in order to make it more readable, it is also possible to gather implementations of
all functions into the `source` section, resulting in the following component declaration:

```toml title="vector.toml"
[transforms.aggregator]
type = "lua"
version = "2"
inputs = [] # add IDs of the input components here
hooks.init = "init"
hooks.process = "process"
hooks.shutdown = "shutdown"
timers = [{interval_seconds = 5, handler = "timer_handler"}]

source = """
  function init()
    count = 0
  end

  function process()
    count = count + 1
  end

  function timer_handler(emit)
    emit(make_counter(counter))
    counter = 0
  end

  function shutdown(emit)
    emit(make_counter(counter))
  end

  function make_counter(value)
    return metric = {
      name = "event_counter",
      kind = "incremental",
      timestamp = os.date("!*t"),
      counter = {
        value = value
      }
    }
  end
"""
```

## A Loadable Module

As the Lua source grows, it becomes beneficial to place it into a separate file. One obvious advantage is the
possibility to use Lua syntax highlighting in the text editor. A less obvious one is the possibility to share
common functionality between different scripted transforms using [loadable modules][urls.lua_modules].

There are many ways to use modules in Lua. The simplest one is to just use [`require`][urls.lua_require] function to
evaluate code from a file, setting up some global variables.

With this approach the config from the previous section becomes split into two files:

```lua title="aggregator.lua"
function init()
  count = 0
end

function aggregator.process()
  count = count + 1
end

function aggregator.timer_handler(emit)
  emit(make_counter(counter))
  counter = 0
end

function aggregator.shutdown(emit)
  emit(make_counter(counter))
end

function aggregator.make_counter(value)
  return metric = {
    name = "event_counter",
    kind = "incremental",
    timestamp = os.date("!*t"),
    counter = {
      value = value
    }
  }
end
```

and

```toml title="vector.toml"
[transforms.aggregator]
type = "lua"
version = "2"
inputs = [] # add IDs of the input components here
hooks.init = "init"
hooks.process = "process"
hooks.shutdown = "shutdown"
timers = [{interval_seconds = 5, handler = "timer_handler"}]
source = "require('aggregator')"
```

There are also [other possibilities][urls.lua_modules_tutorial] to define Lua modules which do not require to use
global variables, but they are not Vector-specific, and so out of scope of this guide.

## Conclusion

As you have witnessed by reading this guide, the power of Vector comes from its flexibility. In addition to
providing a [rich set][docs.transforms] of predefined transforms for building production-grade observability
pipelines, it makes it possible to write custom aggregations as Lua scripts. This allows each role of Vector
in a [deployment topology][docs.setup.deployment.topologies] to perform various kinds of aggregations, providing
alternatives to centralized logs aggregation.

[docs.about.concepts#pipelines]: /docs/about/concepts/#pipeline
[docs.architecture.data-model.metric#counter]: /docs/about/under-the-hood/architecture/data-model/metric/#counter
[docs.setup.deployment.topologies]: /docs/setup/deployment/topologies/
[docs.transforms.lua#event-data-model]: /docs/reference/configuration/transforms/lua/#event-data-model
[docs.transforms.lua#source]: /docs/reference/configuration/transforms/lua/#source
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua/
[docs.transforms]: /docs/reference/configuration/transforms/
[urls.dry_code]: https://en.wikipedia.org/wiki/Don%27t_repeat_yourself
[urls.lua_modules]: https://www.lua.org/manual/5.3/manual.html#6.3
[urls.lua_modules_tutorial]: http://lua-users.org/wiki/ModulesTutorial
[urls.lua_require]: https://www.lua.org/manual/5.3/manual.html#pdf-require
