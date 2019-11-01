---
title: "add_tags transform" 
sidebar_label: "add_tags"
---

The `add_tags` transform accepts [`metric`][docs.data-model.metric] events and allows you to add one or more metric tags.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


```coffeescript
[transforms.my_transform_id]
  # REQUIRED - General
  type = "add_tags" # enum
  inputs = ["my-source-id"]
  
  # REQUIRED - Tags
  [transforms.my_transform_id.tags]
    my_tag = "my value" # example
    my_env_tag = "${ENV_VAR}" # example
```



You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tags"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"table"}
  unit={null}>

### tags

A table of key/value pairs representing the tags to be added to the metric.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"my_tag","value":"my value"},{"name":"my_env_tag","value":"${ENV_VAR}"}]}
  name={"*"}
  nullable={false}
  path={"tags"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"*"}
  unit={null}>

#### *

A key/value pair representing the new tag to be added.


</Option>


</Options>

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

1. Check for any [open `add_tags_transform` issues][urls.add_tags_transform_issues].
2. If encountered a bug, please [file a bug report][urls.new_add_tags_transform_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_add_tags_transform_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.transforms.lua]
* [`remove_tags` transform][docs.transforms.remove_tags]

## Resources

* [**Issues**][urls.add_tags_transform_issues] - [enhancements][urls.add_tags_transform_enhancements] - [bugs][urls.add_tags_transform_bugs]
* [**Source code**][urls.add_tags_transform_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.metric]: ../../../about/data-model/metric.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.transforms.lua]: ../../../usage/configuration/transforms/lua.md
[docs.transforms.remove_tags]: ../../../usage/configuration/transforms/remove_tags.md
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.add_tags_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+add_tags%22+label%3A%22Type%3A+bug%22
[urls.add_tags_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+add_tags%22+label%3A%22Type%3A+enhancement%22
[urls.add_tags_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+add_tags%22
[urls.add_tags_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/add_tags.rs
[urls.new_add_tags_transform_bug]: https://github.com/timberio/vector/issues/new?labels=transform%3A+add_tags&labels=Type%3A+bug
[urls.new_add_tags_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=transform%3A+add_tags&labels=Type%3A+enhancement
[urls.vector_chat]: https://chat.vector.dev
