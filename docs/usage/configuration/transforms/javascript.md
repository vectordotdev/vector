---
description: Accepts `log` events and allows you to transform events with a full embedded JavaScript engine.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/javascript.md.erb
-->

# javascript transform

![][images.javascript_transform]

{% hint style="warning" %}
The `javascript` transform is in beta. Please see the current
[enhancements][url.javascript_transform_enhancements] and
[bugs][url.javascript_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_javascript_transform_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `javascript` transform accepts [`log`][docs.log_event] events and allows you to transform events with a full embedded JavaScript engine.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  type = "javascript" # must be: "javascript"
  inputs = ["my-source-id"]
  
  handler = "handler" # no default
  memory_limit = 10000000 # no default
  path = "/etc/vector/transform.js" # no default
  source = """
event => ({...event, field: 'value'})

"""
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  type = "javascript"
  inputs = ["<string>", ...]
  handler = "<string>"
  memory_limit = <int>
  path = "<string>"
  source = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.javascript_transform]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "javascript"
  type = "javascript"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # Name of the handler function.
  # 
  # * optional
  # * no default
  handler = "handler"

  # Maximum allowed RAM usage for JavaScript engine in bytes.
  # 
  # * optional
  # * no default
  memory_limit = 10000000

  # The path to JavaScript source file with handler function.
  # 
  # * optional
  # * no default
  path = "/etc/vector/transform.js"

  # The inline JavaScript source with handler function.
  # 
  # * optional
  # * no default
  source = """
event => ({...event, field: 'value'})

"""
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "javascript"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **OPTIONAL** | | |
| `handler` | `string` | Name of the handler function. See [Environment Variables](#environment-variables) for more info.<br />`no default` `example: "handler"` |
| `memory_limit` | `int` | Maximum allowed RAM usage for JavaScript engine in bytes.<br />`no default` `example: 10000000` |
| `path` | `string` | The path to JavaScript source file with handler function.<br />`no default` `example: "/etc/vector/transform.js"` |
| `source` | `string` | The inline JavaScript source with handler function. See [Environment Variables](#environment-variables) for more info.<br />`no default` `example: (see above)` |

## Examples

{% tabs %}
{% tab title="Add fields" %}
Add a root field to an event. Supply this as the `source` value:

```js
event => ({...event, field: 'value'})
```

Add a nested field to an event. Supply this as the `source` value:

```js
event => ({...event, ['nested.field']: 'value})
```

{% endtab %}
{% tab title="Remove fields" %}
Remove a field from an event. Supply this as the `source` value:

```js
event => ({...event, field: null})
```

Remove a nested field from an event. Supply this as the `source` value:

```js
event => ({...event, ['nested.field']: null})
```
{% endtab %}

{% tab title="Drop event" %}
Drop an event entirely. Supply this as the `source` value:

```js
event => null
```
{% endtab %}

{% tab title="Generate multiple events" %}
Generate multiple events from a single event. Supply this as the `source` value:

```js
event => [{...event, field1: 'value1'}, {...event, field2: 'value2'}]
```
{% endtab %}

{% tab title="Set event timestamp" %}
Extract date encoded as UNIX timestamp from the message and set event timestamp from it.
Supply this as the `source` value:

```js
event => {
    const {created_at} = JSON.stringify(event.message)
    event.timestamp = new Date(created_at * 1000)
    return event
}
```
{% endtab %}

{% tab title="Keep state between events" %}
Keep variables between processing subsequent events. These variables are recreated
if Vector is restarted.

Supply this as the `source` value:

```js
let count = 0
const handler = event => ({...event, count: ++count})
```

and set value of `handler` parameter to `handler`.
{% endtab %}

{% endtabs %}

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

Vector uses [QuickJS](https://bellard.org/quickjs/quickjs.html) embedded
JavaScript engine for scripted transformations of the events. It implements
ECMAScript 2019 and parts of ECMAScript 2020.

Transformations are done using _handlers_. Handler is a user-defined
JavaScript function that takes one event object as input and outputs either of:

* Event: a JavaScript object with values of types `Boolean`, `Number`, `String`, or `Date`.
  If you need nested objects, use dot-separated key values, for example `a.b.c`.
* Null: if handler returns `null`, the event is discarded.
* Array: an array of event objects.

The handler code is specified in the `source` field of the transform config. If `handler`
parameter of the config is not specified, entire source should be a single function.

If `handler` parameter is specified, the source should contain a definition of the handler
function with the same name as the value of `handler`. In addition, the source can contain
other top-level functions, variables, constants, or classes. The value of `handler`
should consist of ASCII characters and be a valid JavaScript identifier.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `javascript_transform` issues][url.javascript_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_javascript_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_javascript_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`javascript` transform][docs.javascript_transform]
* [`lua` transform][docs.lua_transform]

## Resources

* [**Issues**][url.javascript_transform_issues] - [enhancements][url.javascript_transform_enhancements] - [bugs][url.javascript_transform_bugs]
* [**Source code**][url.javascript_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.javascript_transform]: ../../../usage/configuration/transforms/javascript.md
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.javascript_transform]: ../../../assets/javascript-transform.svg
[url.javascript_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+javascript%22+label%3A%22Type%3A+Bug%22
[url.javascript_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+javascript%22+label%3A%22Type%3A+Enhancement%22
[url.javascript_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+javascript%22
[url.javascript_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/javascript.rs
[url.new_javascript_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+javascript&labels=Type%3A+Bug
[url.new_javascript_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+javascript&labels=Type%3A+Enhancement
[url.new_javascript_transform_issue]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+javascript
[url.vector_chat]: https://chat.vector.dev
