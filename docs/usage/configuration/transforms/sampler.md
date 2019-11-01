---
title: "sampler transform" 
sidebar_label: "sampler"
---

The `sampler` transform accepts [`log`][docs.data-model.log] events and allows you to sample events with a configurable rate.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="simple">

```coffeescript
[transforms.my_transform_id]
  type = "sampler" # enum
  inputs = ["my-source-id"]
  rate = 10
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[transforms.my_transform_id]
  # REQUIRED
  type = "sampler" # enum
  inputs = ["my-source-id"]
  rate = 10
  
  # OPTIONAL
  pass_list = ["[error]", "field2"] # no default
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[["[error]","field2"]]}
  name={"pass_list"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"[string]"}
  unit={null}>

### pass_list

A list of regular expression patterns to exclude events from sampling. If an event's `"message"` key matches _any_ of these patterns it will _not_ be sampled.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[10]}
  name={"rate"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"int"}
  unit={null}>

### rate

The rate at which events will be forwarded, expressed as 1/N. For example, `rate = 10` means 1 out of every 10 events will be forwarded and the rest will be dropped.


</Option>


</Options>

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `sampler_transform` issues][urls.sampler_transform_issues].
2. If encountered a bug, please [file a bug report][urls.new_sampler_transform_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_sampler_transform_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.transforms.lua]

## Resources

* [**Issues**][urls.sampler_transform_issues] - [enhancements][urls.sampler_transform_enhancements] - [bugs][urls.sampler_transform_bugs]
* [**Source code**][urls.sampler_transform_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.transforms.lua]: ../../../usage/configuration/transforms/lua.md
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_sampler_transform_bug]: https://github.com/timberio/vector/issues/new?labels=transform%3A+sampler&labels=Type%3A+bug
[urls.new_sampler_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=transform%3A+sampler&labels=Type%3A+enhancement
[urls.sampler_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+sampler%22+label%3A%22Type%3A+bug%22
[urls.sampler_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+sampler%22+label%3A%22Type%3A+enhancement%22
[urls.sampler_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+sampler%22
[urls.sampler_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/sampler.rs
[urls.vector_chat]: https://chat.vector.dev
