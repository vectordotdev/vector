---
title: Parsing CSV logs with Lua
description: Parse structured application logs in CSV format using Lua transform
authors: ["binarylogic"]
domain: transforms
transforms: ["lua"]
weight: 4
tags: ["lua", "csv", "logs", "transform", "advanced", "guides", "guide"]
---

{{< requirement title="Pre-requisites" >}}

* You understand the <a href="/docs/reference/configuration/transforms/lua">basic Lua concepts</a>.
* You understand the <a href="/docs/about/concepts">basic Vector concepts</a> and understand <a href="/docs/setup/quickstart/">how to set up a pipeline</a>

{{< /requirement >}}

Vector has many built-in [parsers][urls.vector_parsing_transforms] for structured logs formats. However, when you need
to ship logs in a custom or application-specific format, [programmable transforms][urls.vector_programmable_transforms]
have got you covered.

This guide walks through reading CSV logs using [`file`][docs.sources.file] source and parsing them using the [`lua`][docs.transforms.lua] transform with a loadable Lua module.

## Getting Started

For certainty, it is assumed in the following that the logs to be read are produced by
[`csvlog`][urls.postgresql_csvlog] in PostgreSQL. For example, there might be the following
log file:


```csv title="log.csv"
2020-04-09 12:48:49.661 UTC,,,1,,localhost.1,1,,2020-04-09 12:48:49 UTC,,0,LOG,00000,"ending log output to stderr",,"Future log output will go to log destination ""csvlog"".",,,,,,,""
2020-04-09 12:48:49.669 UTC,,,27,,localhost.1b,1,,2020-04-09 12:48:49 UTC,,0,LOG,00000,"database system was shut down at 2020-04-09 12:48:25 UTC",,,,,,,,,""
2020-04-09 12:48:49.683 UTC,,,1,,localhost.1,2,,2020-04-09 12:48:49 UTC,,0,LOG,00000,"database system is ready to accept connections",,,,,,,,,""
```

Let us draft an initial version of the Vector's configuration file:

```toml title="vector.toml"
data_dir = "."

[sources.file]
  type = "file"
  include = ["*.csv"]
  start_at_beginning = true

[transforms.lua]
  inputs = ["file"]
  type = "lua"
  version = "2"
  hooks.process = """
    function (event, emit)
      -- to be expanded
      emit(event)
    end
  """

[sinks.console]
  inputs = ["lua"]
  type = "console"
  encoding.codec = "json"
```

This config sets up a [pipeline][docs.meta.glossary#pipeline] that reads log files, pipes them through the parsing
transform (which currently is configured to just pass the events through), and displays the produced log events using
[`console`][docs.sinks.console] sink.

At this point, running `vector --config vector.toml` results in the following output:

```json
{"file":"log.csv","host":"localhost","message":"2020-04-09 12:48:49.661 UTC,,,1,,localhost.1,1,,2020-04-09 12:48:49 UTC,,0,LOG,00000,\"ending log output to stderr\",,\"Future log output will go to log destination \"\"csvlog\"\".\",,,,,,,\"\"","timestamp":"2020-04-09T14:33:28Z"}
{"file":"log.csv","host":"localhost","message":"2020-04-09 12:48:49.669 UTC,,,27,,localhost.1b,1,,2020-04-09 12:48:49 UTC,,0,LOG,00000,\"database system was shut down at 2020-04-09 12:48:25 UTC\",,,,,,,,,\"\"","timestamp":"2020-04-09T14:33:28Z"}
{"file":"log.csv","host":"localhost","message":"2020-04-09 12:48:49.683 UTC,,,1,,localhost.1,2,,2020-04-09 12:48:49 UTC,,0,LOG,00000,\"database system is ready to accept connections\",,,,,,,,,\"\"","timestamp":"2020-04-09T14:33:28Z"}
```

## Adding the CSV Module

In order to perform actual parsing, it is possible to leverage [`lua-csv`][urls.lua_csv_repo].
Because it consists of a [single file][urls.lua_csv_view], it is possible to just download it to the same
directory where `vector.toml` is stored:

```bash
curl -o csv.lua https://raw.githubusercontent.com/geoffleyland/lua-csv/d20cd42d61dc52e7f6bcb13b596ac7a7d4282fbf/lua/csv.lua
```

Then it would be possible to load it by calling [`require`][urls.lua_require] Lua function in the
[`source`][docs.transforms.lua#source] configuration section:

```toml
source = """
  csv = require("csv")
"""
```

With this `source` the `csv` module is loaded when Vector is started up (or if the `lua` transform is added later and the
config is automatically reloaded) and can be used through the global variable `csv`.

## Implementing Custom Parsing

With the `csv` module, the [`hooks.process`][docs.transforms.lua#process] can be changed to the following:

```toml
hooks.process = """
  function (event, emit)
    fields = csv.openstring(event.log.message):lines()() -- parse the `message` field
    event.log.message = nil -- drop the `message` field

    column_names = {  -- a sequence containing CSV column names
      -- ...
    }

    for column, value in ipairs(fields) do -- iterate over CSV columns
      column_name = column_names[column] -- get column name
      event.log[column_name] = value -- set the corresponding field in the event
    end

    emit(event) -- emit the transformed event
  end
"""
```

Note that the `column_names` can be created just once, in the `source` section instead to speed up processing.
Putting it there and using the column names from the PostgreSQL documentation results in the following definition of
the whole transform:

```toml title="vector.toml"
# ...
[transforms.lua]
  inputs = ["file"]
  type = "lua"
  version = "2"
  source = """
    csv = require("csv") -- load external module for parsing CSV
    column_names = {  -- a sequence containing CSV column names
      "log_time",
      "user_name",
      "database_name",
      "process_id",
      "connection_from",
      "session_id",
      "session_line_num",
      "command_tag",
      "session_start_time",
      "virtual_transaction_id",
      "transaction_id",
      "error_severity",
      "sql_state_code",
      "message",
      "detail",
      "hint",
      "internal_query",
      "internal_query_pos",
      "context",
      "query",
      "query_pos",
      "location",
      "application_name",
    }
  """
  hooks.process = """
    function (event, emit)
      fields = csv.openstring(event.log.message):lines()() -- parse the `message` field
      event.log.message = nil -- drop the `message` field

      for column, value in ipairs(fields) do -- iterate over CSV columns
        column_name = column_names[column] -- get column name
        event.log[column_name] = value -- set the corresponding field in the event
      end

      emit(event) -- emit the transformed event
    end
    """
#...
```

Trying to run `vector --config vector.toml` with the same input file results in structured events being output:

```json
{"application_name":"","command_tag":"","connection_from":"","context":"","database_name":"","detail":"","error_severity":"LOG","file":"log.csv","hint":"Future log output will go to log destination \"csvlog\".","host":"localhost","internal_query":"","internal_query_pos":"","location":"","log_time":"2020-04-09 12:48:49.661 UTC","message":"ending log output to stderr","process_id":"1","query":"","query_pos":"","session_id":"localhost.1","session_line_num":"1","session_start_time":"2020-04-09 12:48:49 UTC","sql_state_code":"00000","timestamp":"2020-04-09T19:49:07Z","transaction_id":"0","user_name":"","virtual_transaction_id":""}
{"application_name":"","command_tag":"","connection_from":"","context":"","database_name":"","detail":"","error_severity":"LOG","file":"log.csv","hint":"","host":"localhost","internal_query":"","internal_query_pos":"","location":"","log_time":"2020-04-09 12:48:49.669 UTC","message":"database system was shut down at 2020-04-09 12:48:25 UTC","process_id":"27","query":"","query_pos":"","session_id":"localhost.1b","session_line_num":"1","session_start_time":"2020-04-09 12:48:49 UTC","sql_state_code":"00000","timestamp":"2020-04-09T19:49:07Z","transaction_id":"0","user_name":"","virtual_transaction_id":""}
{"application_name":"","command_tag":"","connection_from":"","context":"","database_name":"","detail":"","error_severity":"LOG","file":"log.csv","hint":"","host":"localhost","internal_query":"","internal_query_pos":"","location":"","log_time":"2020-04-09 12:48:49.683 UTC","message":"database system is ready to accept connections","process_id":"1","query":"","query_pos":"","session_id":"localhost.1","session_line_num":"2","session_start_time":"2020-04-09 12:48:49 UTC","sql_state_code":"00000","timestamp":"2020-04-09T19:49:07Z","transaction_id":"0","user_name":"","virtual_transaction_id":""}
```

Or, applying pretty formatting to one of the output events:

```json
{
  "application_name": "",
  "command_tag": "",
  "connection_from": "",
  "context": "",
  "database_name": "",
  "detail": "",
  "error_severity": "LOG",
  "file": "log.csv",
  "hint": "Future log output will go to log destination \"csvlog\".",
  "host": "localhost",
  "internal_query": "",
  "internal_query_pos": "",
  "location": "",
  "log_time": "2020-04-09 12:48:49.661 UTC",
  "message": "ending log output to stderr",
  "process_id": "1",
  "query": "",
  "query_pos": "",
  "session_id": "localhost.1",
  "session_line_num": "1",
  "session_start_time": "2020-04-09 12:48:49 UTC",
  "sql_state_code": "00000",
  "timestamp": "2020-04-09T19:49:07Z",
  "transaction_id": "0",
  "user_name": "",
  "virtual_transaction_id": ""
}
```

## Further Improvements

After the task of parsing the CSV logs is accomplished, the following improvements can take place.

### Support for Multi-line Strings

CSV supports line breaks in strings. However, by default `file` source creates a separate event from each line.

There are two options to deal with this:

1. For simple cases it might be possible to use the [`multiline`][docs.sources.file#multiline] configuration
  option in the `file` source.
2. For more complex cases the messages from multiple events can be conditionally concatenated in the Lua code. See
  [the aggregations guide][guides.advanced.custom-aggregations-with-lua] for more details on this.

### Change Fields Types

By default, all columns are parsed as strings. It is possible to convert them to other
[data types][docs.transforms.lua#data-types] right in the Lua code using
built-in functions, such as [`tonumber`][urls.lua_tonumber]. Alternatively, it is possible to add the
[`coercer`][docs.transforms.coercer] transform after the `lua` transform, for example, to
[parse timestamps][docs.transforms.coercer#timestamps].

[docs.meta.glossary#pipeline]: /docs/reference/glossary/#pipeline
[docs.sinks.console]: /docs/reference/configuration/sinks/console/
[docs.sources.file#multiline]: /docs/reference/configuration/sources/file/#multiline
[docs.sources.file]: /docs/reference/configuration/sources/file/
[docs.transforms.coercer]: /docs/reference/vrl/functions/#coerce-functions
[docs.transforms.lua#data-types]: /docs/reference/configuration/transforms/lua/#event-data-model
[docs.transforms.lua#process]: /docs/reference/configuration/transforms/lua/#hooks.process
[docs.transforms.lua#source]: /docs/reference/configuration/transforms/lua/#source
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua/
[guides.advanced.custom-aggregations-with-lua]: /guides/advanced/custom-aggregations-with-lua/
[urls.lua_csv_repo]: https://github.com/geoffleyland/lua-csv
[urls.lua_csv_view]: https://github.com/geoffleyland/lua-csv/blob/09557e4608b02d136b9ae39a8fa0f36328fa1cec/lua/csv.lua
[urls.lua_require]: https://www.lua.org/manual/5.3/manual.html#pdf-require
[urls.lua_tonumber]: https://www.lua.org/manual/5.3/manual.html#pdf-tonumber
[urls.postgresql_csvlog]: https://www.postgresql.org/docs/current/runtime-config-logging.html#RUNTIME-CONFIG-LOGGING-CSVLOG
[urls.vector_parsing_transforms]: /components/?functions%5B%5D=parse
[urls.vector_programmable_transforms]: /components/?functions%5B%5D=program
