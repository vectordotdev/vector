---
date: "2022-01-12"
title: "Vector Config Unit Testing Improvements"
description: "Support for unit testing transforms with multiple outputs"
authors: ["001wwang"]
pr_numbers: []
release: "0.20.0"
hide_on_release_notes: false
---

We've added support for unit testing transforms with multiple outputs as well as
full support for testing task-style transforms (previously there may have been
issues when using multiple inputs with task transforms). For example, you can
now test `remap` transform's `dropped` output like so,

```toml
[transforms.foo]
  type = "remap"
  inputs = []
  drop_on_abort = true
  reroute_dropped = true
  source = "abort"

[[tests]]
  name = "remap_dropped_output"
  no_outputs_from = [ "foo" ]

  [[tests.inputs]]
    insert_at = "foo"
    type = "log"
    [tests.inputs.log_fields]
      message = "I will be dropped"

  [[tests.outputs]]
    extract_from = "foo.dropped"

    [[tests.outputs.conditions]]
      type = "vrl"
      source = 'assert_eq!(.message, "I will be dropped", "incorrect message")'
```

Under-the-hood, we've reworked the unit testing implementation to more closely
align with how a configuration actually runs, making it easier to support
testing new features. However, as a result, some unit testing debug UX has also
changed: previously, on a test condition error, debug output included `input` event
  information representing the event(s) prior to being transformed. `input`
  event information is no longer available. The original `input` event(s) for a
  test can be determined from your configuration.
