---
title: Unit Tests
description: Unit test configuration options
---

It's possible to define unit tests within a Vector configuration file that cover
a network of transforms within the topology. The intention of these tests is to
improve the maintainability of configs containing larger and more complex
combinations of transforms.

Executing tests within a config file can be done with the[`test`](#test) subcommand:

```bash
vector test /etc/vector/*.toml
```

## Configuration

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="common">

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" />

```toml
[[.tests]]
    # REQUIRED - General
    name = "foo test" # example
    
    # REQUIRED - Outputs
    [[.tests.outputs]]
      # REQUIRED - General
      extract_from = "bar" # example
      
      # REQUIRED - Conditions
      [.tests.outputs.conditions]
        [.tests.outputs.conditions.*]
    
    # REQUIRED - Input
    [.tests.input]
      # REQUIRED
      type = "raw" # example, enum
      insert_at = "foo" # example
      
      # OPTIONAL
      value = "some message contents" # example, no default, relevant when type = "raw"
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" />

```toml
[[.tests]]
    # REQUIRED - General
    name = "foo test" # example
    
    # REQUIRED - Outputs
    [[.tests.outputs]]
      # REQUIRED - General
      extract_from = "bar" # example
      
      # REQUIRED - Conditions
      [.tests.outputs.conditions]
        [.tests.outputs.conditions.*]
    
    # REQUIRED - Input
    [.tests.input]
      # REQUIRED - General
      type = "raw" # example, enum
      insert_at = "foo" # example
      
      # OPTIONAL - General
      value = "some message contents" # example, no default, relevant when type = "raw"
      
      # OPTIONAL - Log fields
      [.tests.input.log_fields]
        message = "some message contents" # example
        host = "myhost" # example
      
      # OPTIONAL - Metric
      [.tests.input.metric]
        # REQUIRED - General
        type = "counter" # example, enum
        name = "duration_total" # example
        timestamp = "2019-11-01T21:15:47.443232Z" # example
        val = 10.2 # example
        
        # OPTIONAL - General
        direction = "plus" # example, no default, enum
        sample_rate = 1 # example, no default
        
        # OPTIONAL - Tags
        [.tests.input.metric.tags]
          host = "foohost" # example
          region = "us-east-1" # example
```

</TabItem>

</Tabs>

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tests"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"[table]"}
  unit={null}
  >

### tests

A table that defines a unit test.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["foo test"]}
  name={"name"}
  nullable={false}
  path={"tests"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### name

A unique identifier for this test.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"input"}
  nullable={false}
  path={"tests"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"table"}
  unit={null}
  >

#### input

A table that defines a unit test input event.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["foo"]}
  name={"insert_at"}
  nullable={false}
  path={"tests.input"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

##### insert_at

The name of a transform, the input event will be delivered to this transform inorder to begin the test.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"raw":"Creates a log event where the message contents are specified in the field 'value'.","log":"Creates a log event where log fields are specified in the table 'log_fields'.","metric":"Creates a metric event, where its type and fields are specified in the table 'metric'."}}
  examples={["raw","log","metric"]}
  name={"type"}
  nullable={false}
  path={"tests.input"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

##### type

The event type.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["some message contents"]}
  name={"value"}
  nullable={true}
  path={"tests.input"}
  relevantWhen={{"type":"raw"}}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

##### value

Specifies the log message field contents when the input type is 'raw'.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"log_fields"}
  nullable={true}
  path={"tests.input"}
  relevantWhen={{"type":"log"}}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

##### log_fields

Specifies the log fields when the input type is 'log'.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"message","value":"some message contents"},{"name":"host","value":"myhost"}]}
  name={"*"}
  nullable={false}
  path={"tests.input.log_fields"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"*"}
  unit={null}
  >

###### *

A key/value pair representing a field to be added to the input event.


</Field>


</Fields>

</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"metric"}
  nullable={true}
  path={"tests.input"}
  relevantWhen={{"type":"metric"}}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

##### metric

Specifies the metric type when the input type is 'metric'.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"counter":"A [counter metric type][docs.data-model#counters].","gauge":"A [gauge metric type][docs.data-model#gauges].","histogram":"A [histogram metric type][docs.data-model#histograms].","set":"A [set metric type][docs.data-model#sets]."}}
  examples={["counter"]}
  name={"type"}
  nullable={false}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

###### type

The metric type.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["duration_total"]}
  name={"name"}
  nullable={false}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

###### name

The name of the metric. Defaults to `<field>_total` for `counter` and `<field>` for `gauge`.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tags"}
  nullable={true}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

###### tags

Key/value pairs representing [metric tags][docs.data-model#tags].

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"host","value":"foohost"},{"name":"region","value":"us-east-1"}]}
  name={"*"}
  nullable={false}
  path={"tests.input.metric.tags"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

####### *

Key/value pairs representing [metric tags][docs.data-model#tags].


</Field>


</Fields>

</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[10.2]}
  name={"val"}
  nullable={false}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"float"}
  unit={null}
  >

###### val

Amount to increment/decrement or gauge.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["2019-11-01T21:15:47.443232Z"]}
  name={"timestamp"}
  nullable={false}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

###### timestamp

Time metric was created/ingested.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[1]}
  name={"sample_rate"}
  nullable={true}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"float"}
  unit={null}
  >

###### sample_rate

The bucket/distribution the metric is a part of.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={{"plus":"Increase the gauge","minus":"Decrease the gauge"}}
  examples={["plus","minus"]}
  name={"direction"}
  nullable={true}
  path={"tests.input.metric"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

###### direction

The direction to increase or decrease the gauge value.


</Field>


</Fields>

</Field>


</Fields>

</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"outputs"}
  nullable={false}
  path={"tests"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"[table]"}
  unit={null}
  >

#### outputs

A table that defines a unit test expected output.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["bar"]}
  name={"extract_from"}
  nullable={false}
  path={"tests.outputs"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

##### extract_from

The name of a transform, at the end of the test events extracted from thistransform will be checked against a table of conditions.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"conditions"}
  nullable={false}
  path={"tests.outputs"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"table"}
  unit={null}
  >

##### conditions

A table that defines a collection of conditions to check against the output of atransform. A test is considered to have passed when each condition has resolvedtrue for one or more events extracted from the target transform.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[{"key":"check message is a thing","value":{"type":"check_fields","message.equals":"a thing"}}]}
  name={"*"}
  nullable={false}
  path={"tests.outputs.conditions"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"table"}
  unit={null}
  >

###### *

A key/value pair representing a condition to be checked on the output of atransform. Keys should be an identifier for the condition that gives context asto what it is checking for.


</Field>


</Fields>

</Field>


</Fields>

</Field>


</Fields>

</Field>


</Fields>


[docs.data-model#counters]: /docs/about/data-model#counters
[docs.data-model#gauges]: /docs/about/data-model#gauges
[docs.data-model#histograms]: /docs/about/data-model#histograms
[docs.data-model#sets]: /docs/about/data-model#sets
[docs.data-model#tags]: /docs/about/data-model#tags
