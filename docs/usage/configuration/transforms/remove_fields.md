---
event_types: ["log","log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+remove_fields%22
output_types: ["log"]
sidebar_label: "remove_fields|[\"log\",\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/remove_fields.rs
status: "prod-ready"
title: "remove_fields transform" 
---

The `remove_fields` transform accepts [`log`][docs.data-model.log] events and allows you to remove one or more log fields.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration"/ >

```toml
[transforms.my_transform_id]
  type = "remove_fields" # example, must be: "remove_fields"
  inputs = ["my-source-id"] # example
  fields = ["field1", "field2"] # example
```

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["field1","field2"]]}
  name={"fields"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"[string]"}
  unit={null}>

### fields

The log field names to drop.


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
[docs.data-model.log]: ../../../about/data-model/log.md
