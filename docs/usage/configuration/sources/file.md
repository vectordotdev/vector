---
description: Ingests data through one or more local files and outputs `log` events.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/file.md.erb
-->

# file source

![][images.file_source]

{% hint style="warning" %}
The `file` source is in beta. Please see the current
[enhancements][url.file_source_enhancements] and
[bugs][url.file_source_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_file_source_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `file` source ingests data through one or more local files and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sources.my_source_id]
  # REQUIRED - General
  type = "file" # must be: "file"
  exclude = ["/var/log/nginx/access.log"]
  include = ["/var/log/nginx/*.log"]
  
  # OPTIONAL - General
  data_dir = "/var/lib/vector" # no default
  fingerprint_bytes = 256 # default, bytes
  glob_minimum_cooldown = 1000 # default, milliseconds
  ignore_older = 86400 # no default, seconds
  ignored_header_bytes = 0 # default, bytes
  max_line_bytes = 102400 # default, bytes
  start_at_beginning = false # default
  
  # OPTIONAL - Context
  file_key = "file" # default
  host_key = "host" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sources.<source-id>]
  # REQUIRED - General
  type = "file"
  exclude = ["<string>", ...]
  include = ["<string>", ...]

  # OPTIONAL - General
  data_dir = "<string>"
  fingerprint_bytes = <int>
  glob_minimum_cooldown = <int>
  ignore_older = <int>
  ignored_header_bytes = <int>
  max_line_bytes = <int>
  start_at_beginning = <bool>

  # OPTIONAL - Context
  file_key = "<string>"
  host_key = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sources.file_source]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "file"
  type = "file"

  # Array of file patterns to exclude. Globbing is supported. *Takes precedence
  # over the `include` option.*
  # 
  # * required
  # * no default
  exclude = ["/var/log/nginx/access.log"]

  # Array of file patterns to include. Globbing is supported.
  # 
  # * required
  # * no default
  include = ["/var/log/nginx/*.log"]

  # The directory used to persist file checkpoint positions. By default, the
  # global `data_dir` is used. Please make sure the Vector project has write
  # permissions to this dir.
  # 
  # * optional
  # * no default
  data_dir = "/var/lib/vector"

  # The number of bytes read off the head of the file to generate a unique
  # fingerprint.
  # 
  # * optional
  # * default: 256
  # * unit: bytes
  fingerprint_bytes = 256

  # Delay between file discovery calls. This controls the interval at which
  # Vector searches for files.
  # 
  # * optional
  # * default: 1000
  # * unit: milliseconds
  glob_minimum_cooldown = 1000

  # Ignore files with a data modification date that does not exceed this age.
  # 
  # * optional
  # * no default
  # * unit: seconds
  ignore_older = 86400

  # The number of bytes to skipe ahead (or ignore) when generating a unique
  # fingerprint. This is helpful if all files share a common header.
  # 
  # * optional
  # * default: 0
  # * unit: bytes
  ignored_header_bytes = 0

  # The maximum number of a bytes a line can contain before being discarded. This
  # protects against malformed lines or tailing incorrect files.
  # 
  # * optional
  # * default: 102400
  # * unit: bytes
  max_line_bytes = 102400

  # When `true` Vector will read from the beginning of new files, when `false`
  # Vector will only read new data added to the file.
  # 
  # * optional
  # * default: false
  start_at_beginning = false

  #
  # Context
  #

  # The key name added to each event with the full path of the file.
  # 
  # * optional
  # * default: "file"
  file_key = "file"

  # The key name added to each event representing the current host.
  # 
  # * optional
  # * default: "host"
  host_key = "host"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "file"` |
| `exclude` | `[string]` | Array of file patterns to exclude. [Globbing](#globbing) is supported. *Takes precedence over the `include` option.*<br />`required` `example: ["/var/log/nginx/access.log"]` |
| `include` | `[string]` | Array of file patterns to include. [Globbing](#globbing) is supported.<br />`required` `example: ["/var/log/nginx/*.log"]` |
| **OPTIONAL** - General | | |
| `data_dir` | `string` | The directory used to persist file checkpoint positions. By default, the global `data_dir` is used. Please make sure the Vector project has write permissions to this dir. See [Checkpointing](#checkpointing) for more info.<br />`no default` `example: "/var/lib/vector"` |
| `fingerprint_bytes` | `int` | The number of bytes read off the head of the file to generate a unique fingerprint. See [File Identification](#file-identification) for more info.<br />`default: 256` `unit: bytes` |
| `glob_minimum_cooldown` | `int` | Delay between file discovery calls. This controls the interval at which Vector searches for files. See [Auto Discovery](#auto-discovery) and [Globbing](#globbing) for more info.<br />`default: 1000` `unit: milliseconds` |
| `ignore_older` | `int` | Ignore files with a data modification date that does not exceed this age. See [File Rotation](#file-rotation) for more info.<br />`no default` `example: 86400` `unit: seconds` |
| `ignored_header_bytes` | `int` | The number of bytes to skipe ahead (or ignore) when generating a unique fingerprint. This is helpful if all files share a common header. See [File Identification](#file-identification) for more info.<br />`default: 0` `unit: bytes` |
| `max_line_bytes` | `int` | The maximum number of a bytes a line can contain before being discarded. This protects against malformed lines or tailing incorrect files.<br />`default: 102400` `unit: bytes` |
| `start_at_beginning` | `bool` | When `true` Vector will read from the beginning of new files, when `false` Vector will only read new data added to the file. See [Read Position](#read-position) for more info.<br />`default: false` |
| **OPTIONAL** - Context | | |
| `file_key` | `string` | The key name added to each event with the full path of the file. See [Context](#context) for more info.<br />`default: "file"` |
| `host_key` | `string` | The key name added to each event representing the current host. See [Context](#context) for more info.<br />`default: "host"` |

## Examples

Given the following input:

{% code-tabs %}
{% code-tabs-item title="/var/log/rails.log" %}
```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.log_event] will be emitted with the following structure:

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
[`regex` transform][docs.regex_parser_transform].

## How It Works

### Auto Discovery

Vector will continually look for new files matching any of your include
patterns. The frequency is controlled via the `glob_minimum_cooldown` option. 
If a new file is added that matches any of the supplied patterns, Vector will
begin tailing it. Vector maintains a unique list of files and will not tail a
file more than once, even if it matches multiple patterns. You can read more
about how we identify file in the [Identification](#file-identification)
section.

### Checkpointing

Vector checkpoints the current read position in the file after each successful
read. This ensures that Vector resumes where it left off if restarted,
preventing data from being read twice. The checkpoint positions are stored in
the data directory which is specified via the
[global `data_dir` option][docs.configuration.data-directory] but can be
overridden via the `data_dir` option in the `file` sink directly.

### Context

By default, the `file` source will add context
keys to your events via the `file_key` and `host_key`
options.

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### File Deletions

If a file is deleted Vector will flush the current buffer and stop tailing
the file.

### File Identification

By default, Vector identifies files by creating a [cyclic redundancy check
(CRC)][url.crc] on the first 256 bytes of the file. This serves as a
fingerprint to uniquely identify the file. The amount of bytes read can be
controlled via the `fingerprint_bytes` and `ignored_header_bytes` options.

This strategy avoids the common pitfalls of using device and inode names since
inode names can be reused across files. This enables Vector to [properly tail
files in the event of rotation][docs.correctness].

### File Rotation

Vector will follow files across rotations in the manner of tail, and because of
the way Vector [identifies files](#file-identification), Vector will properly
recognize newly rotated files regardless if you are using `copytruncate` or
`create` directive. To ensure Vector handles rotated files properly we
recommend:

1. Ensure the `includes` paths include rotated files. For example, use
   `/var/log/nginx*.log` to recognize `/var/log/nginx.2.log`.
2. Use either the `copytruncate` or `create` directives when rotating files.
   If historical data is compressed, or altered in any way, Vector will not be
   able to properly identify the file.
3. Only delete files when they have exceeded the `ignore_older` age. While
   extremely rare, this ensures you do not delete data before Vector has a
   chance to ingest it.

### Globbing

[Globbing][url.globbing] is supported in all provided file paths, files will
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
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `file_source` issues][url.file_source_issues].
2. If encountered a bug, please [file a bug report][url.new_file_source_bug].
3. If encountered a missing feature, please [file a feature request][url.new_file_source_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.file_source_issues] - [enhancements][url.file_source_enhancements] - [bugs][url.file_source_bugs]
* [**Source code**][url.file_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.configuration.data-directory]: ../../../usage/configuration#data-directory
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.correctness]: ../../../correctness.md
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.file_source]: ../../../assets/file-source.svg
[url.crc]: https://en.wikipedia.org/wiki/Cyclic_redundancy_check
[url.file_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22+label%3A%22Type%3A+Bug%22
[url.file_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22+label%3A%22Type%3A+Enhancement%22
[url.file_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22
[url.file_source_source]: https://github.com/timberio/vector/tree/master/src/sources/file.rs
[url.globbing]: https://en.wikipedia.org/wiki/Glob_(programming)
[url.new_file_source_bug]: https://github.com/timberio/vector/issues/new?labels=Source%3A+file&labels=Type%3A+Bug
[url.new_file_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=Source%3A+file&labels=Type%3A+Enhancement
[url.new_file_source_issue]: https://github.com/timberio/vector/issues/new?labels=Source%3A+file
[url.vector_chat]: https://chat.vector.dev
