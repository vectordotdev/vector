---
event_types: ["metric","metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+remove_tags%22
output_types: ["metric"]
sidebar_label: "remove_tags|[\"metric\",\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/remove_tags.rs
status: "prod-ready"
title: "remove_tags transform" 
---

The `remove_tags` transform accepts [`metric`][docs.data-model.metric] events and allows you to remove one or more metric tags.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration"/ >

```toml
[transforms.my_transform_id]
  type = "remove_tags" # example, must be: "remove_tags"
  inputs = ["my-source-id"] # example
  tags = ["tag1", "tag2"] # example
```

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["tag1","tag2"]]}
  name={"tags"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"[string]"}
  unit={null}>

### tags

The tag names to drop.


</Option>


</Options>

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.metric]: ../../../about/data-model/metric.md
