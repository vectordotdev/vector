# RFC #1999 - 2020-03-06 - API extensions for `lua` transform

This RFC proposes a new API for the `lua` transform.

## Motivation

Currently, the [`lua` transform](https://vector.dev/docs/reference/transforms/lua/) has some limitations in its API. In particular, the following features are missing:

*   **Nested Fields**

    Currently accessing nested fields is possible using the dot notation:

    ```lua
    event["nested.field"] = 5
    ```

    However, users expect nested fields to be accessible as native Lua structures, for example like this:

    ```lua
    event["nested"]["field"] = 5
    ```

    See [#706](https://github.com/timberio/vector/issues/706) and [#1406](https://github.com/timberio/vector/issues/1406).

*   **Setup Code**

    Some scripts require expensive setup steps, for example, loading of modules or invoking shell commands. These steps should not be part of the main transform code.

    For example, this code adding custom hostname

    ```lua
    if event["host"] == nil then
      local f = io.popen ("/bin/hostname")
      local hostname = f:read("*a") or ""
      f:close()
      hostname = string.gsub(hostname, "\n$", "")
      event["host"] = hostname
    end
    ```

    Should be split into two parts, the first part executed just once at the initialization:

    ```lua
    local f = io.popen ("/bin/hostname")
    local hostname = f:read("*a") or ""
    f:close()
    hostname = string.gsub(hostname, "\n$", "")
    ```

    and the second part executed for each incoming event:

    ```lua
    if event["host"] == nil then
      event["host"] = hostname
    end
    ```

    See [#1864](https://github.com/timberio/vector/issues/1864).

*   **Control Flow**

    It should be possible to define channels for output events, similarly to how it is done in [`swimlanes`](https://vector.dev/docs/reference/transforms/swimlanes/) transform.

    See [#1942](https://github.com/timberio/vector/issues/1942).

## Prior Art

The implementation of `lua` transform has the following design:

* There is a `source` parameter which takes a string of code.
* When a new event comes in, the global variable `event` is set inside the Lua context and the code from `source` is evaluated.
* After that, Vector reads the global variable `event` as the processed event.
* If the global variable `event` is set to `nil`, then the event is dropped.

Events have type [`userdata`](https://www.lua.org/pil/28.1.html) with custom [metamethods](https://www.lua.org/pil/13.html), so they are views to Vector's events. Thus passing an event to Lua has zero cost, so only when fields are actually accessed the data is copied to Lua.

The fields are accessed through string indexes using [Vector's dot notation](https://vector.dev/docs/about/data-model/log/#dot-notation).

## Guide-level Proposal

### Motivating example


```toml
[transforms.lua]
  type = "lua"
  inputs = []
  version = "2" # defaults to 1
  source = """
    counter = counter + 1
    -- without calling `emit` function nothing is produced by default
  """
  [transforms.lua.hooks]
  init = """
    counter = 0
    previous_timestamp = os.time()
    emit({
      log = {
        messge = "starting up",
        timestamp = os.date("!*t),
      }
    Event = Event.new_log()
    event["log"]["message"] = "starting up"
  """
  shutdown = """
    final_stats_event = {
      log = {
        count = counter,
        timestamp = os.date("!*t"),
        interval = os.time() - previous_timestamp
      }
    }
    final_stats_event.log.stats.rate = final_stats_event["log"]["stats"].count / final_stats_event.log.stats.interval
    emit(final_stats_event)

    emit({
      log = {
        message = "shutting down",
        timestamp = os.date("!*t"),
      }
    }, "auxiliary")
  """
  [[transforms.lua.timers]]
  interval = 10
  source = """
    emit {
      metric = {
        name = "response_time_ms",
        timestamp = os.date("!*t"),
        kind = "absolute",
        tags = {
          host = "localhost"
        },
        value = {
          type = "counter",
          value = 24.2
        }
      }
    }
  """
  [[transforms.lua.timers]]
  interval = 60
  source = """
    event = {
      log = {
        message = "heartbeat",
        timestamp = os.date("!*t),
      }
    }
    emit(event, "auxiliary")
  """
```

The code above consumes the incoming events, counts them, and then emits these stats about these counts every 10 seconds. In addition, it sends debug logs about its functioning into a separate lane called `auxiliary`.

### Proposed changes

* Add `version` configuration option which would allow the users to chose between the new API described in this RFC (version 2) and the old one (version 1).
* Hooks for initialization and shutdown called `init` and `shutdown`. They are defined as strings of Lua code in the `hooks` section of the configuration of the transform.
* Timers which define pieces of code that are executed periodically. They are defined in array `timers`, each timer takes two configuration options: `interval` which is the interval for execution in seconds and `source` which is the code which is to be executed periodically.
* Events are produced by the transform by calling function `emit` with the first argument being the event and the second option argument being the name of the lane where to emit the event. Outputting the events by storing them to the `event` global variable should not be supported, so its content would be ignored.
* Support direct access to the nested fields (in both maps and arrays).
* Add support for the timestamp type as a `userdata` object with the same visible fields as in the table returned by [`os.date`](https://www.lua.org/manual/5.3/manual.html#pdf-os.date). In addition, monkey-patch `os.date` function available inside Lua scripts to make it return the same kind of userdata instead of a table if it is called with `*t` or `!*t` as the argument. This is necessary to allow one-to-one correspondence between types in Vector and Lua.

## Sales Pitch

The proposal

* gives users more power to create custom transforms;
* supports both logs and metrics;
* makes it possible to add complexity to the configuration of the transform gradually only when needed.

## Drawbacks

The only drawback is that supporting both dot notation and classical indexing makes it impossible to add escaping of dots in field names. For example, for incoming event structure like

```json
{
  "field.first": {
    "second": "value"
  }
}
```

accessing `event["field.first"]` would return `nil`.

However, because of the specificity of the observability data, there seems to be no need to have both field names with dots and nested fields.

## Outstanding Questions

* Should timestamps be automatically inserted to created logs and metrics created as tables inside the transform is they are not present?
* Are there better alternatives to the proposed solution for supporting of the timestamp type?
* Could some users be surprised if the transform which doesn't call `emit` function doesn't output anything?
* `null` might present in the events would be lost because in Lua setting a field to `nil` means deletion. Is it acceptable? If it is not, it is possible to introduce a new kind of `userdata` for representing `null` values.

## Plan of Action

- [ ] Implement support for `version` config option and split implementations for versions 1 and 2.
- [ ] Implement access to the nested structure of logs events.
- [ ] Support creation of logs events as table inside the transform.
- [ ] Implement metrics support.
- [ ] Add `emit` function.
- [ ] Add `init` and `shutdown` hooks.
- [ ] Add timers.
- [ ] Implement support for the timestamp type compatible with the result of execution of `os.date("!*t")`.
