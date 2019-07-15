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
The `file` sink is in beta. Please see the current
[enhancements][url.file_source_enhancements] and
[bugs][url.file_source_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_file_source_issues]
as it will help shape the roadmap of this component.
{% endhint %}

The `file` source ingests data through one or more local files and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sources.my_file_source_id]
  # REQUIRED - General
  type = "file" # must be: "file"
  exclude = ["/var/log/nginx/access.log"]
  include = ["/var/log/nginx/*.log"]
  
  # OPTIONAL - General
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
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "file"` |
| `exclude` | `[string]` | Array of file patterns to exclude. [Globbing](#globbing) is supported. *Takes precedence over the `include` option.*<br />`required` `example: ["/var/log/nginx/access.log"]` |
| `include` | `[string]` | Array of file patterns to include. [Globbing](#globbing) is supported.<br />`required` `example: ["/var/log/nginx/*.log"]` |
| **OPTIONAL** - General | | |
| `fingerprint_bytes` | `int` | The number of bytes read off the head of the file to generate a unique fingerprint. See [File Identification](#file-identification) for more info.<br />`default: 256` `unit: bytes` |
| `glob_minimum_cooldown` | `int` | Delay between file discovery calls. This controls the interval at which Vector searches for files.<br />`default: 1000` `unit: milliseconds` |
| `ignore_older` | `int` | Ignore files with a data modification date that does not exceed this age. See [  If historical data is compressed, or altered in any way, Vector will not be](#if-historical-data-is-compressed-or-altered-in-any-way-vector-will-not-be) for more info.<br />`no default` `example: 86400` `unit: seconds` |
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
patterns. If a new file is added that matches any of the supplied patterns,
Vector will begin tailing it. Vector maintains a unique list of files and will
not tail a file more than once, even if it matches multiple patterns. You can
read more about how we identify file in the Identification section.

### Context

Each event is augmented with contextual fields controlled by the `file_key`
and `host_key` options. Please see the descriptions for each respective option.

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
be [autodiscovered](#auto_discovery) continually.

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) or `EOF` is found.

### Read Position

Vector defaults to reading new data only. Only data added to the file after
Vector starts tailing the file will be collected. To read from the beginning
of the file set the `start_at_beginning` option to true.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.file_source_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.file_source_issues] - [enhancements][url.file_source_enhancements] - [bugs][url.file_source_bugs]
* [**Source code**][url.file_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.correctness]: ../../../correctness.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.file_source]: ../../../assets/file-source.svg
[url.community]: https://vector.dev/community
[url.crc]: https://en.wikipedia.org/wiki/Cyclic_redundancy_check
[url.file_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22+label%3A%22Type%3A+Bug%22
[url.file_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22+label%3A%22Type%3A+Enhancement%22
[url.file_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+file%22
[url.file_source_source]: https://github.com/timberio/vector/tree/master/src/sources/file.rs
[url.globbing]: https://en.wikipedia.org/wiki/Glob_(programming)
[url.new_file_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+new_file%22
[url.search_forum]: https://forum.vector.dev/search?expanded=true
