---
title: Merge multi-line logs with Lua
description: Combine multi-line CSV rows into single events using the Lua transform
authors: ["binarylogic"]
domain: transforms
transforms: ["lua"]
weight: 3
tags: ["lua", "merge", "multiline", "multi-line", "advanced", "guides", "guide"]
---

{{< requirement title="Pre-requisites" >}}

* You understand the [basic Lua concepts][docs.transforms.lua].
* You understand the [basic Vector concepts][docs.about.concepts] and understand [how to set up a basic pipeline][docs.setup.quickstart].
* You know how to [parse CSV logs with Lua][guides.parsing-csv-logs-with-lua].

[docs.about.concepts]: /docs/about/concepts
[docs.setup.quickstart]: /docs/setup/quickstart
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua
[guides.parsing-csv-logs-with-lua]: /guides/advanced/parsing-csv-logs-with-lua
{{< /requirement >}}

The [guide to parsing CSV logs with Lua][guides.parsing-csv-logs-with-lua] describes how to parse CSV logs containing
values that don't contain line breaks. According to [RFC 4180][urls.rfc_4180], however, CSV values enclosed in double
quotes *can* contain line breaks. This means that parsing arbitrary CSV logs requires handling such line breaks
correctly.

This can't be accomplished using the [`multiline` option of the `file` source][docs.sources.file#multiline] because it uses regular expressions for delimiting lines, and for the given use case a full-fledged CSV parser is necessary.

## A Minimal Example

It's possible to merge CSV log lines using the same [`lua-csv`][urls.lua_csv_repo] module used
in the [guide on parsing CSV logs][guides.parsing-csv-logs-with-lua]. The underlying algorithm is the following:

1. Parse incoming log line as a CSV row.
2. Check the number of fields in it.
   1. If the number of fields matches the expected number of fields,
      then the log line contains all necessary fields and can be
      processed further.
   2. Otherwise, store the log line in the state of the transform and
      then, when the next event comes, merge the subsequent log line
      with the previous ones and repeat parsing again.

Such an algorithm can be implemented, for example, with the following transform config:

```toml title="vector.toml"
[transforms.lua]
  inputs = []
  type = "lua"
  version = "2"
  source = """
    csv = require("csv") -- load the `lua-csv` module
    expected_columns = 23 -- expected number of columns in incoming CSV lines
    line_separator = "\\r\\n" -- note the double escaping required by the TOML format
  """
  hooks.process = """
    function (event, emit)
      merged_event = merge(event)
      if merged_event == nil then -- a global variable containing the merged event
        merged_event = event -- if it is empty, set it to the current event
      else -- otherwise, concatenate the line in the stored merged event
           -- with the next line
        merged_event.log.message = merged_event.log.message ..
                                  line_separator .. event.log.message
      end

      fields = csv.openstring(event.log.message):lines()() -- parse CSV
      if #fields < expected_columns then
        return -- not all fields are present in the merged event yet
      end

      -- do something with the array of the parsed fields
      merged_event.log.csv_fields = fields -- for example, just store them in an
                                           -- array field

      emit(merged_event) -- emit the resulting event
      merged_event = nil -- clear the merged event
    end
  """
```

In this code sample, the `source` option defines code that's executed when the transform is created.
while the `hooks.process` option defines a function that's called for each incoming event.

## How It Works

The merging process is shown in this diagram:

{{< svg "/img/guides/merge-transform.svg" >}}

The `lua` transform has internal state, which can be accessed and modified from user-defined code
using global variables. Initially, the state is empty, which corresponds to `merged_event` variable
being set to `nil`.

As events arrive to the transform, they cause the `merged_event` variable to hold an aggregated
event, thus making the event non-empty.

In the end, when the state holds enough data to extract all fields, a merged event is emitted and
the state is emptied. Then the process repeats as new events arrive.

## Safety Checks

The merging algorithm used above is simple and would work for data coming from trusted sources. However,
in general case it might happen that the CSV is malformed, so that some field is not terminated by `"`,
which can cause unbounded growth of the `message` field. In order to prevent this, it is possible to replace
the following lines

```lua
merged_event.log.message = merged_event.log.message ..
                           line_separator .. event.log.message
```

in the definition of the `process` hook by this code:

```lua
merged_event = safe_merge(merged_event, event)
if not merged_event then
  return
end
```

and add the following definition of the `safe_merge` function to the [`source`][docs.transforms.lua#source]
section of the config:

```lua
function safe_merge(merged_event, event)
  if #merged_event.log.message + #event.log.message > 4096 then
    return nil
  else
    merged_event.log.message = merged_event.log.message ..
                               line_separator .. event.log.message
    return merged_event
  end
end
```

This function checks whether the total length of merged lines not larger than 4096 (the actual value can be made
larger if it is necessary by a particular use case) and, if that is the case, performs actual merging.

In general, it is recommended to always add such safety checks to the code of your custom transforms in order to
ensure that malformed input would not cause memory leaks or other kinds of undesired behavior.

## Further Steps

After the problem of merging multi-line logs in custom formats is solved, you might be interested
in checking out the following guides:

* [Unit Testing Your Configs][guides.unit-testing]
* [Custom Aggregations with Lua][guides.advanced.custom-aggregations-with-lua]

[docs.about.concepts]: /docs/about/concepts/
[docs.setup.quickstart]: /docs/setup/quickstart/
[docs.sources.file#multiline]: /docs/reference/configuration/sources/file/#multiline
[docs.transforms.lua#source]: /docs/reference/configuration/transforms/lua/#source
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua/
[guides.advanced.custom-aggregations-with-lua]: /guides/advanced/custom-aggregations-with-lua/
[guides.parsing-csv-logs-with-lua]: /guides/advanced/parsing-csv-logs-with-lua/
[guides.unit-testing]: /guides/level-up/unit-testing/
[urls.lua_csv_repo]: https://github.com/geoffleyland/lua-csv
[urls.rfc_4180]: https://tools.ietf.org/html/rfc4180
