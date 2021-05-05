---
title: JournalD
description: Collect logs from [JournalD](https://www.freedesktop.org/software/systemd/man/systemd-journald.service.html)
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Checkpointing

{{< snippet "checkpointing" >}}

### Communication strategy

To ensure that the `journald` source works across all platforms, Vector interacts with the Systemd journal via the [`journalctl`][journalctl] command. This is accomplished by spawning a subprocess that Vector interacts with. If the `journalctl` command isn't in the environment path you can specify the exact location via the [`journalctl_path`][journalctl_path] option. For more information on this communication strategy, please see [issue 1473][issue_1473].

### Context

{{< snippet "context" >}}

### Non-ASCII messages

When `journald` has stored a message that isn't strict ASCII, it outputs it in an alternate format to prevent data loss. Vector handles this alternative format by translating such messages into UTF-8 in "lossy" mode, where characters that are not valid UTF-8 are replaced with the Unicode replacement character (`�`).

### State

{{< snippet "stateless" >}}

[data_dir]: /docs/reference/configuration/global-options/#data_dir
[issue_1473]: https://github.com/timberio/vector/issues/1473
[journalctl]: https://vector.dev/docs/reference/configuration/sources/journald/#journalctl
[journalctl_path]: https://vector.dev/docs/reference/configuration/sources/journald/#journalctl_path
