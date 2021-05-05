---
title: File
description: Collect logs from [files](https://en.wikipedia.org/wiki/File_system)
kind: source
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Autodiscovery

Vector continually looks for new files matching any of your included patterns. The frequency of that is controlled via the [`glob_minimum_cooldown_ms`](#glob_minimum_cooldown_ms) option. If a new file is added that matches any of the supplied patterns, Vector begins tailing it. Vector maintains a unique list of files and doesn't tile a file more than once, even if it matches multiple patterns.

### Checkpointing

{{< snippet "checkpointing" >}}

### Compressed files

Vector transparently detects files that have been compressed using Gzip and decompresses them for reading. This detection process looks for the unique sequence of bytes in the Gzip header and doesn't rely on the compressed files adhering to any kind of naming convention.

One caveat with reading compressed files is that Vector isn't able to efficiently seek into them. Rather than implementing a potentially expensive full scan as a seek mechanism, Vector currently doesn't attempt to make further reads from a file for which it has already stored a checkpoint in a previous run. For this reason, users should be sure to allow Vector to fully process any compressed files before shutting the process down or moving the files to another location on disk.

### Context

{{< snippet "context" >}}

### File detection

When a watched file is deleted, Vector maintains its open file handle and continuse reading until it reaches `EOF`. When a file is no longer findable in the [`include`](#include) option and the reader has reached `EOF`, that file's reader is discarded.

### File read order

By default, Vector attempts to allocate its read bandwidth fairly across all of the files it's currently watching. This prevents a single very busy file from starving other independent files from being read. In certain situations, however, this can lead to interleaved reads from files that should be read one after the other.

For example, consider a service that logs to timestamped file, creating a new one at an interval and leaving the old one as-is. Under normal operation, Vector would follow writes as they happen to each file and there would be no interleaving. In an overload situation, however, Vector may pick up and begin tailing newer files before catching up to the latest writes from older files. This would cause writes from a single logical log stream to be interleaved in time and potentially slow down ingestion as a whole, since the fixed total read bandwidth is allocated across an increasing number of files.

To address this type of situation, Vector provides the [`oldest_first`](#oldest_first) option. When set, Vector doesn't read from any file younger than the oldest file that it hasn't yet caught up to. In other words, Vector continues reading from older files as long as there is more data to read. Only once it hits the end does it move on to reading from younger files.

Whether to use the `oldest_first` flag depends on the organization of the logs you're configuring Vector to tail. If your `include` option contains multiple independent logical log streams (e.g. Nginx's `access.log` and `error.log` or logs from multiple services), you are likely better off with the default behavior. If you're dealing with a single logical log stream or if you value per-stream ordering over fairness across streams, consider setting the `oldest_first` option to `true`.

### File rotation

Vector supports tailing across a number of file rotation strategies. The default behavior of `logrotate` is simply to move the old log file and create a new one. This requires no special configuration of Vector, as it maintains its open file handle to the rotated log until it has finished reading and it finds the newly created file normally.

A popular alternative strategy is `copytruncate`, in which `logrotate` copies the old log file to a new location before truncating the original. Vector also handles this well out of the box, but there are a few configuration options that could help reduce the very small chance of missed data in some edge cases. We recommend a combination of `delaycompress` (if applicable) on the `logrotate` side and including the first rotated file in Vector's `include` option. This allows Vector to find the file after rotation, read it uncompressed to identify it, and then ensure it has all of the data, including any written in a gap between Vector's last read and the actual rotation event.

### File permissions

To be able to source events from the files, Vector must be able to read the files and execute their parent directories.

If you have deployed Vector as using one our distributed packages, then you will find Vector running as the `vector` user. You should ensure that this user has read access to the desired files used as `include`. Strategies for this include:

* Create a new unix group, make it the group owner of the target files, with read access, and add vector to that group
* Use [POSIX ACLs][acl] to grant access to the files to the vector user
* Grant the `CAP_DAC_READ_SEARCH` [Linux capability][linux_capability]. This capability bypasses the file system permissions checks to allow Vector to read any file. This is not recommended as it gives Vector more permissions than it requires, but it is recommended over running Vector as root which would grant it even broader permissions. This can be granted via SystemD by creating an override file using `systemctl edit vector` and adding:

  ```
  AmbientCapabilities=CAP_DAC_READ_SEARCH
  CapabilityBoundingSet=CAP_DAC_READ_SEARCH
  ```

On Debian-based distributions, the `vector` user is automatically added to the [`adm` group][adm_group], if it exists, which has permissions to read `/var/log`.

### Globbing

[Globbing] is supported in all provided file paths. Files are autodiscovered continuously at a rate defined by the [`glob_minimum_cooldown_ms`](#glob_minimum_cooldown_ms) option.

### Line delimiters

{{< snippet "line-delimiters" >}}

### Multiline messages

Sometimes a single log event appears as multiple log lines. To handle this, Vector provides a set of [`multiline`](#multiline) options. These options were carefully thought through and will allow you to solve the simplest and most complex cases. Let's look at a few examples.

#### Example 1: Ruby exceptions

Ruby exceptions, when logged, consist of multiple lines:

```ruby
foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)
    from foobar.rb:6:in `bar'
    from foobar.rb:2:in `foo'
    from foobar.rb:9:in `<main>'
```

To consume these lines as a single event, use the following Vector configuration:

```toml
[sources.my_file_source]
type = "file"
# ...

[sources.my_file_source.multiline]
start_pattern = '^[^\s]'
mode = "continue_through"
condition_pattern = '^[\s]+from'
timeout_ms = 1000
```

* [`start_pattern`](#start_pattern), set to `^[^\s]`, tells Vector that new multi-line events shouldn't start with whitespace.
* [`mode`](#mode), set to `continue_through`, tells Vector to continue aggregating lines until the [`condition_pattern`](#condition_pattern) is no longer valid (excluding the invalid line).
* [`condition_pattern`](#condition_pattern), set to `^[\s]+from`, tells Vector to continue aggregating lines if they start with white-space followed by `from`.

#### Example 2: line continuations

Some programming languages use the backslash (`\`) character to signal that a line will continue on the next line:

```
First line\
second line\
third line
```

To consume these lines as a single event, use the following Vector configuration:

```toml
[sources.my_file_source]
type = "file"
# ...

[sources.my_file_source.multiline]
start_pattern = '\\$'
mode = "continue_past"
condition_pattern = '\\$'
timeout_ms = 1000
```

* [`start_pattern`](#start_pattern), set to `\\$`,  tells Vector that new multi-line events start with lines that end in `\`.
* [`mode`](#mode), set to `continue_past`, tells Vector to continue aggregating lines, plus one additional line, until `condition_pattern` is `false`.
* [`condition_pattern`](#condition_pattern), set to `\\$`, tells Vector to continue aggregating lines if they end with a `\` character.

#### Example 3: line continuations

Activity logs from services such as Elasticsearch typically begin with a timestamp, followed by information on the specific activity, as in this example:

```
[2015-08-24 11:49:14,389][ INFO ][env                      ] [Letha] using [1] data paths, mounts [[/
(/dev/disk1)]], net usable_space [34.5gb], net total_space [118.9gb], types [hfs]
```

To consume these lines as a single event, use the following Vector configuration:

```toml
[sources.my_file_source]
type = "file"
# ...

[sources.my_file_source.multiline]
start_pattern = '^\[[0-9]{4}-[0-9]{2}-[0-9]{2}'
mode = "halt_before"
condition_pattern = '^\[[0-9]{4}-[0-9]{2}-[0-9]{2}'
timeout_ms = 1000
```

* [`start_pattern`](#start_pattern), set to `^\[[0-9]{4}-[0-9]{2}-[0-9]{2}`, tells Vector that new multi-line events start with a timestamp sequence.
* [`mode`](#mode), set to `halt_before`, tells Vector to continue aggregating lines as long as the [`condition_pattern`](#condition_pattern) doesn't match.
* [`condition_pattern`](#condition_pattern), set to `^\[[0-9]{4}-[0-9]{2}-[0-9]{2}`, tells Vector to continue aggregating until a line starts with a timestamp sequence.

### Read position

By default, Vector reads from the beginning of newly discovered files. You can change this behavior by setting the [`read_from`](#read_from) option to `"end"`.

Previously discovered files will be [checkpointed](#checkpointing), and the read position will resume from the last checkpoint. To disable this behavior, you can set the [`ignore_checkpoints`](#ignore_checkpoints) option to `true`. This causes Vector to disregard existing checkpoints when determining the starting read position of a file.

### State

{{< snippet "stateless" >}}

### Fingerprinting

By default, Vector identifies files by create a [cyclic redundancy check][crc] (CRC) on the first 256 bytes of the file. This serves as a fingerprint to uniquely identify the file. The number of bytes read can be controlled via the `fingerprint_bytes` and [`ignored_header_bytes`](#ignore_header_bytes) options.

This strategy avoids the common pitfalls of using device and inode names since inode names can be reused across files. This enables Vector to properly tail files across various rotation strategies.

[acl]: https://www.usenix.org/legacy/publications/library/proceedings/usenix03/tech/freenix03/full_papers/gruenbacher/gruenbacher_html/main.html
[adm_group]: https://wiki.debian.org/SystemGroups
[crc]: https://en.wikipedia.org/wiki/Cyclic_redundancy_check#:~:text=A%20cyclic%20redundancy%20check%20(CRC,polynomial%20division%20of%20their%20contents.
[data_dir]: /docs/reference/configuration/global-options/#data_dir
[globbing]: https://en.wikipedia.org/wiki/Glob_(programming)
[linux_capability]: https://man7.org/linux/man-pages/man7/capabilities.7.html
