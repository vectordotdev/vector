---
description: Accepts `log` events and allows you to transform events with a full embedded Lua engine.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/lua.md.erb
-->

# lua transform

![][images.lua_transform]

{% hint style="warning" %}
The `lua` transform is in beta. Please see the current
[enhancements][url.lua_transform_enhancements] and
[bugs][url.lua_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_lua_transform_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `lua` transform accepts [`log`][docs.log_event] events and allows you to transform events with a full embedded [Lua][url.lua] engine.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  type = "lua" # must be: "lua"
  inputs = ["my-source-id"]
  source = """
require("script") # a `script.lua` file must be in your `search_dirs`

if event["host"] == nil then
  local f = io.popen ("/bin/hostname")
  local hostname = f:read("*a") or ""
  f:close()
  hostname = string.gsub(hostname, "\n$", "")
  event["host"] = hostname
end
"""

  
  search_dirs = ["/etc/vector/lua"] # no default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  type = "lua"
  inputs = ["<string>", ...]
  source = "<string>"
  search_dirs = ["<string>", ...]
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.lua_transform]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "lua"
  type = "lua"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The inline Lua source to evaluate.
  # 
  # * required
  # * no default
  source = """
require("script") # a `script.lua` file must be in your `search_dirs`

if event["host"] == nil then
  local f = io.popen ("/bin/hostname")
  local hostname = f:read("*a") or ""
  f:close()
  hostname = string.gsub(hostname, "\n$", "")
  event["host"] = hostname
end
"""


  # A list of directories search when loading a Lua file via the `require`
  # function.
  # 
  # * optional
  # * no default
  search_dirs = ["/etc/vector/lua"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "lua"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `source` | `string` | The inline Lua source to evaluate. See [Global Variables](#global-variables) for more info.<br />`required` `example: (see above)` |
| **OPTIONAL** | | |
| `search_dirs` | `[string]` | A list of directories search when loading a Lua file via the `require` function. See [Search Directories](#search-directories) for more info.<br />`no default` `example: ["/etc/vector/lua"]` |

## Examples

{% tabs %}
{% tab title="Add fields" %}
Add a field to an event. Supply this as a the `source` value:

```lua
# Add root level field
event["new_field"] = "new value"

# Add nested field
event["parent.child"] = "nested value"
```

{% endtab %}
{% tab title="Remove fields" %}
Remove a field from an event. Supply this as a the `source` value:

```lua
# Remove root level field
event["field"] = nil

# Remove nested field
event["parent.child"] = nil
```

{% endtab %}
{% tab title="Drop event" %}
Drop an event entirely. Supply this as a the `source` value:

```lua
# Drop event entirely
event = nil
```

{% endtab %}
{% endtabs %}

## How It Works

### Dropping Events

To drop events, simply set the `event` variable to `nil`. For example:

```lua
if event["message"].match(str, "debug") then
  event = nil
end
```

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Global Variables

When evaluating the provided `source`, Vector will provide a single global
variable representing the event:

| Name    |           Type           | Description                                                                                                                                                                       |
|:--------|:------------------------:|:----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `event` | [`table`][url.lua_table] | The current [`log` event]. Depending on prior processing the structure of your event will vary. Generally though, it will follow the [default event schema][docs.default_schema]. |

Note, a Lua `table` is an associative array. You can read more about
[Lua types][url.lua_types] in the [Lua docs][url.lua_docs].

### Nested Fields

As described in the [Data Model document][docs.data_model], Vector flatten
events, representing nested field with a `.` delimiter. Therefore, adding,
accessing, or removing nested fields is as simple as added a `.` in your key
name:

```lua
# Add nested field
event["parent.child"] = "nested value"

# Remove nested field
event["parent.child"] = nil
```

### Search Directories

Vector provides a `search_dirs` option that allows you to specify absolute
paths that will searched when using the [Lua `require`
function][url.lua_require].

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `lua_transform` issues][url.lua_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_lua_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_lua_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.lua_transform]

## Resources

* [**Issues**][url.lua_transform_issues] - [enhancements][url.lua_transform_enhancements] - [bugs][url.lua_transform_bugs]
* [**Source code**][url.lua_transform_source]
* [**Lua Reference Manual**][url.lua_manual]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.data_model]: ../../../about/data-model
[docs.default_schema]: ../../../about/data-model/log.md#default-schema
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.lua_transform]: ../../../assets/lua-transform.svg
[url.lua]: https://www.lua.org/
[url.lua_docs]: https://www.lua.org/manual/5.3/
[url.lua_manual]: http://www.lua.org/manual/5.1/manual.html
[url.lua_require]: http://www.lua.org/manual/5.1/manual.html#pdf-require
[url.lua_table]: https://www.lua.org/manual/2.2/section3_3.html
[url.lua_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+lua%22+label%3A%22Type%3A+Bug%22
[url.lua_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+lua%22+label%3A%22Type%3A+Enhancement%22
[url.lua_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+lua%22
[url.lua_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/lua.rs
[url.lua_types]: https://www.lua.org/manual/2.2/section3_3.html
[url.new_lua_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+lua&labels=Type%3A+Bug
[url.new_lua_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+lua&labels=Type%3A+Enhancement
[url.new_lua_transform_issue]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+lua
[url.vector_chat]: https://chat.vector.dev
