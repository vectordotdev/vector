---
title: Lua
kind: transform
---

## Warnings

{{< component/warnings >}}

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Event data model

The [`process`](#process) hook takes an `event` as its first argument. Events are represented as [tables][lua_table] in Lua and follow Vector's data model exactly. Please refer to Vector's [data model reference][data_model] for the event schema. How Vector's types map to Lua's type are covered below.

#### Type mappings

The correspondence between Vector's data types and Lua [data type][log_types] is summarized in this table:

Vector type | Lua type | Comment
:-----------|:---------|:-------
[`String`][vector_string] | [`string`][lua_string] |
[`Integer`][vector_int] | [`integer`][lua_int] |
[`Float`][vector_float] | [`number`][lua_number] |
[`Boolean`][vector_bool] | [`boolean`][lua_bool] |
[`Timestamp`][vector_timestamp] | [`table`][lua_table] | There is no dedicated timestamp type in Lua. Timestamps are represented as tables using the convention defined by [`os.date`][os_date] and [`os.time`][os_time]. The table representation of a timestamp contains the fields `year`, `month`, `day`, `hour`, `min`, `sec`, `nanosec`, `yday`, `wday`, and `isdst`. If such a table is passed from Lua to Vector, the fields `yday`, `wday`, and `isdst` can be omitted. In addition to the `os.time` representation, Vector supports sub-second resolution with a `nanosec` field in the table.
[`Null`][vector_null] | empty string | In Lua setting the value of a table field to `nil` means deletion of this field. In addition, the length operator `#` doesn't work in the expected way with sequences containing nulls. Because of that, `Null` values are encoded as empty strings.
[`Map`][vector_map] | [`table`][lua_table] |
[`Array`][vector_array] | [`sequence`][lua_sequence] | Sequences are a special case of tables. Indices start from 1, following the Lua convention.

### Learning Lua

In order to write non-trivial transforms in Lua, one has to have basic understanding of Lua. Because Lua is an easy to learn language, reading a few first chapters of the [official book][lua_book] or consulting the [manual][lua_manual] would suffice.

### Search directories

Vector provides a [`search_dirs`](#search_dirs) option that enables you to specify absolute paths to search when using the [Lua `require` function][lua_require]. If this option isn't set, the directories of the configuration files are used instead.

### State

{{< snippet "stateless" >}}

[data_model]: /docs/about/under-the-hood/architecture/data-model
[log_types]: /docs/about/data-model/log/#types
[lua_book]: https://www.lua.org/pil/
[lua_bool]: https://www.lua.org/pil/2.2.html
[lua_int]: https://docs.rs/rlua/latest/rlua/type.Integer.html
[lua_manual]: https://www.lua.org/manual/5.3/manual.html
[lua_number]: https://docs.rs/rlua/latest/rlua/type.Number.html
[lua_require]: https://www.lua.org/manual/5.3/manual.html#pdf-require
[lua_sequence]: https://www.lua.org/pil/11.1.html
[lua_string]: https://www.lua.org/pil/2.4.html
[lua_table]: https://www.lua.org/pil/2.5.html
[os_date]: https://www.lua.org/manual/5.3/manual.html#pdf-os.date
[os_time]: https://www.lua.org/manual/5.3/manual.html#pdf-os.time
[vector_array]: /docs/about/data-model/log/#arrays
[vector_bool]: /docs/about/data-model/log/#booleans
[vector_float]: /docs/about/data-model/log/#floats
[vector_int]: /docs/about/data-model/log/#ints
[vector_map]: /docs/about/data-model/log/#maps
[vector_null]: /docs/about/data-model/log/#null-values
[vector_string]: /docs/about/data-model/log/#strings
[vector_timestampe]: /docs/about/data-model/log/#timestamps
