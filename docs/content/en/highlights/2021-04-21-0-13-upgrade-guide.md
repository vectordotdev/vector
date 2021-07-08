---
date: "2020-04-21"
title: "0.13 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.13.0"
authors: ["jszwedko"]
pr_numbers: []
release: "0.13.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

0.13 includes one minor breaking change:

1. [`parse_regex` in VRL no longer returns numeric capture groups by default.](#second)

We cover each below to help you upgrade quickly:

## Upgrade Guide

### Breaking: `parse_regex` in VRL no longer returns numeric capture groups by default.<a name="second"></a>

Previously, when the [Vector Remap Language (VRL)][vrl] [`parse_regex`][parse_regex] function was used, it would return
both named capture groups as well as numeric capture groups.

For example:

```text
parse_regex!("hello 123 world", r'hello (?P<number>\d+) world')
```

Would return:

```json
{ "0": "hello 123 world", "1": "123", "number": "123" }
```

With `0` matching the whole regex, and `1` matching the first capture group, in addition to `number`.

We heard from users that they did not expect the numeric groups by default so we decided to leave them out by default
now.

Using our previous example:

```text
parse_regex!("hello 123 world", r'hello (?P<number>\d+) world')
```

It now returns:

```json
{ "number": "123" }
```

A new `numeric_groups` parameter that can be used to have the numeric capture groups returned like before.

Again using the same example, but with the new parameter:

```text
parse_regex!("hello 123 world", r'hello (?P<number>\d+) world', numeric_groups: true)
```

This returns the old value of:

```json
{ "0": "hello 123 world", "1": "123", "number": "123" }
```

[vrl]: /docs/reference/vrl/
[parse_regex]: /docs/reference/vrl/functions/#parse_regex
