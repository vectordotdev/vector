

---
description: Accepts `log` and `metric` events and allows you to filter events by a field's value.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/field_filter.md.erb
-->

# field_filter transform

![][images.field_filter_transform]

{% hint style="warning" %}
The `field_filter` sink is in beta. Please see the current
[enhancements][url.field_filter_transform_enhancements] and
[bugs][url.field_filter_transform_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_field_filter_transform_issues]
as it will help shape the roadmap of this component.
{% endhint %}

The `field_filter` transform accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to filter events by a field's value.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_field_filter_transform_id]
  # REQUIRED
  type = "field_filter" # must be: "field_filter"
  inputs = ["my-source-id"]
  field = "file"
  value = "/var/log/nginx.log"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED
  type = "field_filter"
  inputs = ["<string>", ...]
  field = "<string>"
  value = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.field_filter]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "field_filter"
  type = "field_filter"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The target field to compare against the `value`.
  # 
  # * required
  # * no default
  field = "file"

  # If the value of the specified `field` matches this value then the event will
  # be permitted, otherwise it is dropped.
  # 
  # * required
  # * no default
  value = "/var/log/nginx.log"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `enum: "field_filter"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `field` | `string` | The target field to compare against the `value`.<br />`required` `example: "file"` |
| `value` | `string` | If the value of the specified `field` matches this value then the event will be permitted, otherwise it is dropped.<br />`required` `example: "/var/log/nginx.log"` |

## How It Works



### Complex Comparisons

The `field_filter` transform is designed for simple equality filtering, it is
not designed for complex comparisons. There are plans to build a `filter`
transform that accepts more complex filtering.

We've opened [issue 479][url.issue_479] for more complex filtering.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.field_filter_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.field_filter_transform_issues] - [enhancements][url.field_filter_transform_enhancements] - [bugs][url.field_filter_transform_bugs]
* [**Source code**][url.field_filter_transform_source]