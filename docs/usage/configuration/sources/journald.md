---
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+journald%22
output_types: ["log"]
sidebar_label: "journald|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/journald.rs
status: "beta"
title: "journald source" 
---

The `journald` source ingests data through log records from journald and outputs [`log`][docs.data-model.log] events.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="common">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration"/ >

```toml
[sources.my_source_id]
  # REQUIRED
  type = "journald" # example, must be: "journald"
  
  # OPTIONAL
  units = ["ntpd", "sysinit.target"] # default
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED
  type = "journald" # example, must be: "journald"
  
  # OPTIONAL
  current_runtime_only = true # default
  data_dir = "/var/lib/vector" # example, no default
  local_only = true # default
  units = ["ntpd", "sysinit.target"] # default
```

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"current_runtime_only"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"bool"}
  unit={null}>

### current_runtime_only

Include only entries from the current runtime (boot)


</Option>


<Option
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["/var/lib/vector"]}
  name={"data_dir"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### data_dir

The directory used to persist the journal checkpoint position. By default, the global `data_dir` is used. Please make sure the Vector project has write permissions to this dir. 


</Option>


<Option
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"local_only"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"bool"}
  unit={null}>

### local_only

Include only entries from the local system


</Option>


<Option
  common={true}
  defaultValue={[]}
  enumValues={null}
  examples={[["ntpd","sysinit.target"]]}
  name={"units"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"[string]"}
  unit={null}>

### units

The list of units names to monitor. If empty or not present, all units are accepted. Unit names lacking a `"."` will have `".service"` appended to make them a valid service unit name.


</Option>


</Options>

## Input/Output

Given the following journald record:

{% code-tabs %}
{% code-tabs-item title="journald record" %}

```
__REALTIME_TIMESTAMP=1564173027000443
__MONOTONIC_TIMESTAMP=98694000446
_BOOT_ID=124c781146e841ae8d9b4590df8b9231
SYSLOG_FACILITY=3
_UID=0
_GID=0
_CAP_EFFECTIVE=3fffffffff
_MACHINE_ID=c36e9ea52800a19d214cb71b53263a28
_HOSTNAME=lorien.example.com
PRIORITY=6
_TRANSPORT=stdout
_STREAM_ID=92c79f4b45c4457490ebdefece29995e
SYSLOG_IDENTIFIER=ntpd
_PID=2156
_COMM=ntpd
_EXE=/usr/sbin/ntpd
_CMDLINE=ntpd: [priv]
_SYSTEMD_CGROUP=/system.slice/ntpd.service
_SYSTEMD_UNIT=ntpd.service
_SYSTEMD_SLICE=system.slice
_SYSTEMD_INVOCATION_ID=496ad5cd046d48e29f37f559a6d176f8
MESSAGE=reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model.log] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <2019-07-26T20:30:27.000443Z>,
  "message": "reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s",
  "host": "lorien.example.com",
  "__REALTIME_TIMESTAMP": "1564173027000443",
  "__MONOTONIC_TIMESTAMP": "98694000446",
  "_BOOT_ID": "124c781146e841ae8d9b4590df8b9231",
  "SYSLOG_FACILITY": "3",
  "_UID": "0",
  "_GID": "0",
  "_CAP_EFFECTIVE": "3fffffffff",
  "_MACHINE_ID": "c36e9ea52800a19d214cb71b53263a28",
  "PRIORITY": "6",
  "_TRANSPORT": "stdout",
  "_STREAM_ID": "92c79f4b45c4457490ebdefece29995e",
  "SYSLOG_IDENTIFIER": "ntpd",
  "_PID": "2156",
  "_COMM": "ntpd",
  "_EXE": "/usr/sbin/ntpd",
  "_CMDLINE": "ntpd: [priv]",
  "_SYSTEMD_CGROUP": "/system.slice/ntpd.service",
  "_SYSTEMD_UNIT": "ntpd.service",
  "_SYSTEMD_SLICE": "system.slice",
  "_SYSTEMD_INVOCATION_ID": "496ad5cd046d48e29f37f559a6d176f8"
}
```

Vector extracts the `"MESSAGE"` field as `"message"`, `"_HOSTNAME"` as
`"host"`, and parses `"_SOURCE_REALTIME_TIMESTAMP"` into `"timestamp"`. All
other fields from journald are kept intact from the source record. You can
further parse the `"message"` key with a [transform][docs.transforms], such as
the [`regex_parser` transform][docs.transforms.regex_parser].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
