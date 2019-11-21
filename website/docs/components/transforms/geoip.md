---

event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+geoip%22
sidebar_label: "geoip|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/geoip.rs
status: "prod-ready"
title: "geoip transform" 
---

The `geoip` transform accepts [`log`][docs.data-model#log] events and allows you to enrich events with geolocation data from the MaxMind GeoIP2 database.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[transforms.my_transform_id]
  # REQUIRED
  type = "geoip" # example, must be: "geoip"
  inputs = ["my-source-id"] # example
  field = "/path/to/database" # example
  
  # OPTIONAL
  target = "default_geoip_target_field" # default
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/database"]}
  name={"field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### field

Path to the MaxMind GeoIP2 database file.


</Field>


<Field
  common={true}
  defaultValue={"default_geoip_target_field"}
  enumValues={null}
  examples={["default_geoip_target_field"]}
  name={"target"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

### target

TODO: fill me in


</Field>


</Fields>

## How It Works

### Database

The `geoip` transform uses the Maxmind Geoip 2 database.

TODO: Fill in with:

1. How is the data enriched?
2. Which fields are added?
3. What happens if a look up fails?
### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#log]: /docs/about/data-model#log
