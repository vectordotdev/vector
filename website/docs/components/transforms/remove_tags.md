---

event_types: ["metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+remove_tags%22
sidebar_label: "remove_tags|[\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/remove_tags.rs
status: "prod-ready"
title: "remove_tags transform" 
---

The `remove_tags` transform accepts [`metric`][docs.data-model#metric] events and allows you to remove one or more metric tags.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[transforms.my_transform_id]
  type = "remove_tags" # example, must be: "remove_tags"
  inputs = ["my-source-id"] # example
  tags = ["tag1", "tag2"] # example
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["tag1","tag2"]]}
  name={"tags"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"[string]"}
  unit={null}
  >

### tags

The tag names to drop.


</Field>


</Fields>

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#metric]: /docs/about/data-model#metric
