# RFC #1999 - 2020-03-06 - API extensions for `lua` transform

This RFC proposes a new API for the `lua` transform.

## Motivation

Currently the [`lua` transform](https://vector.dev/docs/reference/transforms/lua/) has some limitations in its API. In particular, the following features are missing:

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
  source = """
    counter = counter + 1
    event = nil
  """
  [transforms.lua.hooks]
  init = """
    counter = 0
    previous_timestamp = os.time()
    Event = Event.new_log()
    event["message"] = "starting up"
    event:set_lane("auxiliary")
  """
  shutdown = """
    final_stats_event = Event.new_log()
    final_stats_event["stats"] = { count = counter, interval = os.time() - previous_timestamp }
    final_stats_event["stats.rate"] = final_stats_event["stats"].count / final_stats_event["stats.interval"]

    shutdown_event = Event.new_log()
    shutdown_event["message"] = "shutting down"
    shutdown_event:set_lane("auxiliary")

    event = {final_stats_event, shutdown_event}
  """
  [[transforms.lua.timers]]
  interval = 10
  source = """
    event = Event.new_log()
    event["stats"] = { count = counter, interval = 10 }
    event["stats.rate"] = event["stats"].count / event["stats.interval"]
    counter = 0
    previous_timestamp = os.time()
  """
  [[transforms.lua.timers]]
  interval = 60
  source = """
    event["message"] = "heartbeat"
    event:set_lane("auxiliary")
  ""
```

The code above consumes the incoming events, counts them, and then emits these stats about these counts every 10 seconds. In addition, it sends debug logs about its functioning into a separate lane called `auxiliary`.

### Proposed changes

* Hooks for initialization and shutdown called `init` and `shutdown`. They are defined as strings of Lua code in the `hooks` section of the configuration of the transform.
* Timers which define pieces of code that are executed periodically. They are defined in array `timers`, each timer takes two configuration options: `interval` which is the interval for execution in seconds and `source` which is the code which is to be executed periodically.
* Support for setting the output lane using `set_lane` method on the event which takes a string as the parameter. It should also be possible to read the lane using `get_lane` method. Reading from the lanes can be done in the downstream sinks by specifying the name of transform suffixed by a dot and the name of the lane.
* Support multiple output events by making it possible to set the `event` global variable to an [sequence](https://www.lua.org/pil/11.1.html) of events.
* Support direct access to the nested fields (in both maps and arrays).

## Sales Pitch

The proposal

* gives users more power to create custom transforms;
* does not break backward compatibility (except `pairs` method in case of nested fields);
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

* In access to the arrays should the indexes be 0-based or 1-based? Vector uses 0-based indexing, while in Lua the indexing is traditionally 1-based. However, technically it is possible to implement 0-based indexing for arrays which are stored inside events, as both [`__index`](https://www.lua.org/pil/13.4.1.html) and [`__len`](https://www.lua.org/manual/5.3/manual.html#3.4.7) need to have custom implementations in any case.

* Is it confusing that the same global variable name `event` used also for outputting multiple events? The alternative, using a different name, for example, `events`, would lead to questions of precedence in case if both `event` and `events` are set.

## Plan of Action

- [ ] Add `init` and `shutdown` hooks.
- [ ] Add timers.
- [ ] Implement `set_lane` and `get_lane` methods on the events.
- [ ] Support multiple output events.
- [ ] Implement `Event.new_log()` function.
- [ ] Support direct access to the nested fields in addition to the dot notation.
