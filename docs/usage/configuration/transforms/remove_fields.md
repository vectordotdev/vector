

---
description: Accepts `log` and `metric` events and allows you to remove one or more event fields.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/remove_fields.md.erb
-->

# remove_fields transform

![][images.remove_fields_transform]


The `remove_fields` transform accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to remove one or more event fields.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_remove_fields_transform_id]
  # REQUIRED
  type = "remove_fields" # must be: "remove_fields"
  inputs = ["my-source-id"]
  fields = ["field1", "field2"]
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED
  type = "remove_fields"
  inputs = ["<string>", ...]
  fields = ["<string>", ...]
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.remove_fields]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "remove_fields"
  type = "remove_fields"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The field names to drop.
  # 
  # * required
  # * no default
  fields = ["field1", "field2"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `enum: "remove_fields"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `fields` | `[string]` | The field names to drop.<br />`required` `example: ["field1", "field2"]` |

## How It Works



## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.remove_fields_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.


### Alternatives

Finally, consider the following alternatives:


* [`add_fields` transform][docs.add_fields_transform]

## Resources

* [**Issues**][url.remove_fields_transform_issues] - [enhancements][url.remove_fields_transform_enhancements] - [bugs][url.remove_fields_transform_bugs]
* [**Source code**][url.remove_fields_transform_source]