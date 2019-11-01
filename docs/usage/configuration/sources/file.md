---
title: "file source" 
sidebar_label: "file"
---

The `file` source ingests data through one or more local files and outputs [`log`][docs.data-model.log] events.

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
[sources.my_source_id]
  type = "file" # enum
  include = ["/var/log/nginx/*.log"]
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sources.my_source_id]
  # REQUIRED - General
  type = "file" # enum
  include = ["/var/log/nginx/*.log"]
  
  # OPTIONAL - General
  data_dir = "/var/lib/vector" # no default
  exclude = ["/var/log/nginx/access.log"] # no default
  glob_minimum_cooldown = 1000 # default, milliseconds
  ignore_older = 86400 # no default, seconds
  max_line_bytes = 102400 # default, bytes
  start_at_beginning = true # default
  
  # OPTIONAL - Context
  file_key = "file" # default
  host_key = "host" # default
  
  # OPTIONAL - Multi-line
  message_start_indicator = "^(INFO|ERROR)" # no default
  multi_line_timeout = 1000 # default, milliseconds
  
  # OPTIONAL - Priority
  max_read_bytes = 2048 # default, bytes
  oldest_first = true # default
  
  # OPTIONAL - Fingerprinting
  [sources.my_source_id.fingerprinting]
    strategy = "checksum" # default, enum
    fingerprint_bytes = 256 # default, bytes, relevant when strategy = "checksum"
    ignored_header_bytes = 0 # default, bytes, relevant when strategy = "checksum"
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
  examples={["/var/lib/vector"]}
  name={"data_dir"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### data_dir

The directory used to persist file checkpoint positions. By default, the [global `data_dir` option][docs.configuration#data_dir] is used. Please make sure the Vector project has write permissions to this dir. See [Checkpointing](#checkpointing) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[["/var/log/nginx/access.log"]]}
  name={"exclude"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"[string]"}
  unit={null}>

### exclude

Array of file patterns to exclude. [Globbing](#globbing) is supported. *Takes precedence over the [`include` option](#include).*


</Option>


<Option
  defaultValue={"file"}
  enumValues={null}
  examples={["file"]}
  name={"file_key"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### file_key

The key name added to each event with the full path of the file. See [Context](#context) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"fingerprinting"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### fingerprinting

Configuration for how the file source should identify files.

<Options filters={false}>


<Option
  defaultValue={"checksum"}
  enumValues={{"checksum":"Read `fingerprint_bytes` bytes from the head of the file to uniquely identify files via a checksum.","device_and_inode":"Uses the [device and inode][urls.inode] to unique identify files."}}
  examples={["checksum","device_and_inode"]}
  name={"strategy"}
  nullable={true}
  path={"fingerprinting"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### strategy

The strategy used to uniquely identify files. This is important for [checkpointing](#checkpointing) when file rotation is used.


</Option>


<Option
  defaultValue={256}
  enumValues={null}
  examples={[256]}
  name={"fingerprint_bytes"}
  nullable={false}
  path={"fingerprinting"}
  relevantWhen={{"strategy":"checksum"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

#### fingerprint_bytes

The number of bytes read off the head of the file to generate a unique fingerprint. See [File Identification](#file-identification) for more info.


</Option>


<Option
  defaultValue={0}
  enumValues={null}
  examples={[0]}
  name={"ignored_header_bytes"}
  nullable={false}
  path={"fingerprinting"}
  relevantWhen={{"strategy":"checksum"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

#### ignored_header_bytes

The number of bytes to skip ahead (or ignore) when generating a unique fingerprint. This is helpful if all files share a common header. See [File Identification](#file-identification) for more info.


</Option>


</Options>

</Option>


<Option
  defaultValue={1000}
  enumValues={null}
  examples={[1000]}
  name={"glob_minimum_cooldown"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"milliseconds"}>

### glob_minimum_cooldown

Delay between file discovery calls. This controls the interval at which Vector searches for files. See [Auto Discovery](#auto-discovery) and [Globbing](#globbing) for more info.


</Option>


<Option
  defaultValue={"host"}
  enumValues={null}
  examples={["host"]}
  name={"host_key"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### host_key

The key name added to each event representing the current host. See [Context](#context) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[86400]}
  name={"ignore_older"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### ignore_older

Ignore files with a data modification date that does not exceed this age.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[["/var/log/nginx/*.log"]]}
  name={"include"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"[string]"}
  unit={null}>

### include

Array of file patterns to include. [Globbing](#globbing) is supported. See [File Read Order](#file-read-order) and [File Rotation](#file-rotation) for more info.


</Option>


<Option
  defaultValue={102400}
  enumValues={null}
  examples={[102400]}
  name={"max_line_bytes"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### max_line_bytes

The maximum number of a bytes a line can contain before being discarded. This protects against malformed lines or tailing incorrect files.


</Option>


<Option
  defaultValue={2048}
  enumValues={null}
  examples={[2048]}
  name={"max_read_bytes"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### max_read_bytes

An approximate limit on the amount of data read from a single file at a given time.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["^(INFO|ERROR)"]}
  name={"message_start_indicator"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### message_start_indicator

When present, Vector will aggregate multiple lines into a single event, using this pattern as the indicator that the previous lines should be flushed and a new event started. The pattern will be matched against entire lines as a regular expression, so remember to anchor as appropriate.


</Option>


<Option
  defaultValue={1000}
  enumValues={null}
  examples={[1000]}
  name={"multi_line_timeout"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"milliseconds"}>

### multi_line_timeout

When `message_start_indicator` is present, this sets the amount of time Vector will buffer lines into a single event before flushing, regardless of whether or not it has seen a line indicating the start of a new message.


</Option>


<Option
  defaultValue={false}
  enumValues={null}
  examples={[true,false]}
  name={"oldest_first"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### oldest_first

Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from younger files. See [File Read Order](#file-read-order) for more info.


</Option>


<Option
  defaultValue={false}
  enumValues={null}
  examples={[true,false]}
  name={"start_at_beginning"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### start_at_beginning

When `true` Vector will read from the beginning of new files, when `false` Vector will only read new data added to the file. See [Read Position](#read-position) for more info.


</Option>


</Options>

## Input/Output

Given the following input:

{% code-tabs %}
{% code-tabs-item title="/var/log/rails.log" %}
```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model.log] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <timestamp> # current time,
  "message": "2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1",
  "file": "/var/log/rails.log", # original file
  "host": "10.2.22.122" # current nostname
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The `"timestamp"`, `"file"`, and `"host"` keys were automatically added as
context. You can further parse the `"message"` key with a
[transform][docs.transforms], such as the
[`regex_parser` transform][docs.transforms.regex_parser].

## How It Works

### Auto Discovery

Vector will continually look for new files matching any of your include
patterns. The frequency is controlled via the `glob_minimum_cooldown` option.
If a new file is added that matches any of the supplied patterns, Vector will
begin tailing it. Vector maintains a unique list of files and will not tail a
file more than once, even if it matches multiple patterns. You can read more
about how we identify files in the [Identification](#file-identification)
section.

### Checkpointing

Vector checkpoints the current read position in the file after each successful
read. This ensures that Vector resumes where it left off if restarted,
preventing data from being read twice. The checkpoint positions are stored in
the data directory which is specified via the
[global `data_dir` option][docs.configuration#data-directory] but can be
overridden via the `data_dir` option in the `file` sink directly.

### Context

By default, the `file` source will add context
keys to your events via the `file_key` and `host_key`
options.

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### File Deletion

When a watched file is deleted, Vector will maintain its open file handle and
continue reading until it reaches EOF. When a file is no longer findable in the
`includes` glob and the reader has reached EOF, that file's reader is discarded.

### File Identification

By default, Vector identifies files by creating a [cyclic redundancy check
(CRC)][urls.crc] on the first 256 bytes of the file. This serves as a
fingerprint to uniquely identify the file. The amount of bytes read can be
controlled via the `fingerprint_bytes` and `ignored_header_bytes` options.

This strategy avoids the common pitfalls of using device and inode names since
inode names can be reused across files. This enables Vector to [properly tail
files across various rotation strategies][docs.correctness].

### File Read Order

By default, Vector attempts to allocate its read bandwidth fairly across all of
the files it's currently watching. This prevents a single very busy file from
starving other independent files from being read. In certain situations,
however, this can lead to interleaved reads from files that should be read one
after the other.

For example, consider a service that logs to timestamped file, creating
a new one at an interval and leaving the old one as-is. Under normal operation,
Vector would follow writes as they happen to each file and there would be no
interleaving. In an overload situation, however, Vector may pick up and begin
tailing newer files before catching up to the latest writes from older files.
This would cause writes from a single logical log stream to be interleaved in
time and potentially slow down ingestion as a whole, since the fixed total read
bandwidth is allocated across an increasing number of files.

To address this type of situation, Vector provides the `oldest_first` flag. When
set, Vector will not read from any file younger than the oldest file that it
hasn't yet caught up to. In other words, Vector will continue reading from older
files as long as there is more data to read. Only once it hits the end will it
then move on to read from younger files.

Whether or not to use the `oldest_first` flag depends on the organization of the
logs you're configuring Vector to tail. If your `include` glob contains multiple
independent logical log streams (e.g. nginx's `access.log` and `error.log`, or
logs from multiple services), you are likely better off with the default
behavior. If you're dealing with a single logical log stream or if you value
per-stream ordering over fairness across streams, consider setting
`oldest_first` to `true`.

### File Rotation

Vector supports tailing across a number of file rotation strategies. The default
behavior of `logrotate` is simply to move the old log file and create a new one.
This requires no special configuration of Vector, as it will maintain its open
file handle to the rotated log until it has finished reading and it will find
the newly created file normally.

A popular alternative strategy is `copytruncate`, in which `logrotate` will copy
the old log file to a new location before truncating the original. Vector will
also handle this well out of the box, but there are a couple configuration options
that will help reduce the very small chance of missed data in some edge cases.
We recommend a combination of `delaycompress` (if applicable) on the logrotate
side and including the first rotated file in Vector's `include` option. This
allows Vector to find the file after rotation, read it uncompressed to identify
it, and then ensure it has all of the data, including any written in a gap
between Vector's last read and the actual rotation event.

### Globbing

[Globbing][urls.globbing] is supported in all provided file paths, files will
be [autodiscovered](#auto-discovery) continually at a rate defined by the
`glob_minimum_cooldown` option.

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) or `EOF` is found.

### Read Position

By default, Vector will read new data only for newly discovered files, similar
to the `tail` command. You can read from the beginning of the file by setting
the `start_at_beginning` option to `true`.

Previously discovered files will be [checkpointed](#checkpointing), and the
read position will resume from the last checkpoint.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `file_source` issues][urls.file_source_issues].
2. If encountered a bug, please [file a bug report][urls.new_file_source_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_file_source_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.file_source_issues] - [enhancements][urls.file_source_enhancements] - [bugs][urls.file_source_bugs]
* [**Source code**][urls.file_source_source]


[docs.configuration#data-directory]: ../../../usage/configuration#data-directory
[docs.configuration#data_dir]: ../../../usage/configuration#data_dir
[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.correctness]: ../../../correctness.md
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.crc]: https://en.wikipedia.org/wiki/Cyclic_redundancy_check
[urls.file_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+file%22+label%3A%22Type%3A+bug%22
[urls.file_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+file%22+label%3A%22Type%3A+enhancement%22
[urls.file_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+file%22
[urls.file_source_source]: https://github.com/timberio/vector/tree/master/src/sources/file.rs
[urls.globbing]: https://en.wikipedia.org/wiki/Glob_(programming)
[urls.inode]: https://en.wikipedia.org/wiki/Inode
[urls.new_file_source_bug]: https://github.com/timberio/vector/issues/new?labels=source%3A+file&labels=Type%3A+bug
[urls.new_file_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=source%3A+file&labels=Type%3A+enhancement
[urls.vector_chat]: https://chat.vector.dev
