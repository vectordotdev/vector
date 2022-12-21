# RFC 8288 - 2021-07-20 - CSV Enrichment

Vector needs to allow users to enrich the events flowing through the topology
using a CSV file. This RFC proposes adding a VRL function that can perform
this lookup.

## Scope

The RFC will cover a simple VRL function that will lookup a single row from a
CSV file using a set of conditions. This allows the user to map the data into
the event using the full power of VRL.

For MVP purposes, lookup will be a simple sequential scan. The first version
of this function will not handle any indexing of this data. Above a certain size
indexing will provide substantial performance benefits, so it is something that
we should definitely consider at a future stage. Therefore, it is out of scope
for this RFC.

Encryption will not be covered. This is very much a requirement to be considered
at a future stage, but is out of scope for this RFC.

Likewise, reloading the enrichment data is likely to be needed, but will not be
covered here. For the first implementation of this RFC, reloading the data will
require Vector to be restarted.

## Motivation

Users would like to enrich events flowing through Vector with extra data
provided by a CSV file.

## User Experience

### EnrichmentTables

To represent the CSV file we have a new top level configuration option.

```toml
[enrichment_tables.csv_file]
  type = "file"
  encoding.codec = "csv"
  path = "\path_to_csv"
  delimiter = ","
```

The fields available for this section are:

#### type

The type of data that the resource represents. Currently only a csv file is
supported, but this may expand in future to handle other file formats and
database connections.

#### path

The path to the csv file, either an absolute path or one relative to the current
working directory.

#### delimiter

The delimiter used in the csv file to separate fields. Defaults to `","`.

#### header_row

If true, it assumes the first row in the csv file contains column names. If
false, columns are named according to their numerical index.

Note, this is likely to be where any encryption parameters such as the key are
specified when it comes time to implementing encryption.

The initial implementation will only be supporting CSV files as a resource. It
is anticipated that future work will expand the available resources to include
other file types as well as databases.

Like the services that sources and sinks integrate with, enrichment tables will
likely need administrator approval and setup. Typically data is enriched from
some sort of shared company "resource" that end users will likely not have
access to. Admin will likely want to restrict access to resources to approved
pipelines, especially if sensitive information is contained.

The enrichment tables will need to be integrated with `vector validate`, which
would ensure the resource exists and is correctly formatted.

### Schema

For the CSV table all columns will be considered to be Strings. Since enrichment
tables are loaded before VRL compilation it will be possible to ensure that Vrl
doesn't search on columns that do not exist within the datafile. Searching on a
column that doesn't exist can prevent Vector from loading.

If (when these features are implemented) the user attempts to reload a table
and a column that has been Indexed no longer exists, this should prevent the
file from being loaded. Vector will continue to use the currently loaded data.

These restrictions apply only to columns that VRL is searching on. If the user
attempts to use a field that has not been returned from the enrichment, this
will be possible, but the value will be Null. This is largely due to the way
that VRL works with paths into Objects.

### Vrl functions

A remap function:

#### `find_table_row`

This function will look up a single row within the table dataset. If a single
row is found that data is returned as an object, otherwise this function will
error.

A metric will be emitted to indicate the lookup time.

#### Parameters

##### table

The name of the enrichment table to lookup. This must point to a table specified
in the config file eg `enrichment_tables.csv_file`. Both functions are generic
over all table types.

##### condition

`condition` is a single level, key/value object that specifies the fields and
values to lookup. The fields must all match (AND) for the row to be returned.

##### case_sensitive

By default the search will be case insensitive, but this can be changed by
passing `true` to this parameter.

### Example config

```toml
[enrichment_tables.csv_file]
    type = "csv"
    file = "/path/to/csv.csv"
    delimiter = ","

[sources.datadog_logs]
    type = "datadog_logs"
    address = "0.0.0.0:80"

[transforms.simple_enrich]
    type = "remap"
    inputs = ["datadog_logs"]
    source = '''
        . = parse_json!(.message)

        result, err = find_table_row(
            enrichment_tables.csv_file,
            { "license_plate": .license }
        )

        if is_null(err) {
            .first_name = result.first_name
            .last_name = result.last_name
        }
   '''
```

## Implementation

We will need to add a new component type to Vector, call it `EnrichmentTable`.
On loading the Vector config these instances will be created and will load the
data that they are pointing to.

The entire data file will be loaded into memory upon starting Vector, so all
lookups will be performed in memory. An `EnrichmentTable` will need to provide
threadsafe, readonly access to the data that it loads.

VRL will need to maintain the concept of `EnrichmentTable`. This can be created
as an additional element to the `vrl::Value` type. On compilation, the available
tables can be added to the Variable type definitions. This ensures that
during compilation the functions will access valid tables.

### Indexing

Although the initial MVP version is not going to be doing any indexing into the
data it is worth bearing it in mind as we will most likely need to add this in
due course.

In order to perform the indexing VRL needs to know which fields to index. The
criteria is being passed in as an object. If the type def for that object is
known at runtime, we can extract the fields from this.

If the type def isn't known the VRL compiler will raise an error.

Since the table is loaded outside of VRL whilst determining which keys need
indexing occurs inside VRL, we will need a way for VRL to indicate to the table
which indexes need building.

Actual indexing strategies can be decided later.

## Rationale

There is significant customer demand for this feature.

Since enrichment tables are likely to contain sensitive information, creating
enrichment table as a separate section in the config will allow administrators
to configure enrichment tables separately and thus restrict access to approved
pipelines only.

Being a top level configuration option allows the data to be loaded separately
from VRL, this provides cleaner opportunities to provide for encryption and
reloading. Since the data source becomes an orthogonal concept to VRL we can
add features and new data sources to enrichment tables without any impact on
VRL. VRL can transparently swap datasources in and out.

Multiple transforms can share a single table, providing faster load time and
more efficient memory usage.

## Drawbacks

1. There are a number of issues with parsing CSV files in particular around
   handling delimiters and separators. CSV is a less precise format compared to
   JSON and others.
2. Performance. Whilst we can ensure all the IO is performed at boot time,
   searching through that data could still be quite expensive.
3. Memory use. Since the data file is loaded into memory, the data will use up
   some memory. If the file is large enough that could have an impact should
   Vector be running in a constrained environment.
4. Adding a new top level config option creates additional complexity and will
   require some work to the topology.

## Alternatives

### Create a Join Transform

We could create a new type of transform called a Join Transform. This Transform
would be able to take two inputs:

- The Event source
  The data containing the events to be enriched.

- The Enrichment source
  This source would contain the enrichment data. It needs to be loaded in
  before the event data so that all events pass through enriched.

The join transform can be configured to specify the keys to use to join the two
streams.

The advantages of this is that it allows us to reuse the sources that we already
have. The user isn't limited to the data source that we provide as a Resource.

Downsides are that there is a data race at the start. The transform needs some
way of determining that all the necessary data has been loaded from the
Enrichment source before it starts to accept data from the Event source. This
can be tricky to get right.

Another complication could be determining a way to update the Enrichment data.
New records could simply be streamed in by appending to the file, but modified
data could only be updated if we ensured the data also had some form of primary
key.

Because of these, it is felt the Join transform doesn't fit the exact problem of
enrichment we are currently solving, however there are many other scenarios
where this could be useful. In future, we can create a new enrichment table type
that can accept an input from a Vector source which would provide all the
benefits of the Join transform.

### Use a predicate for the search

Instead of using an object to specify the search criteria we could allow the
user to specify a predicate to determine the row to use for enrichment.

```coffee
find_table_row(table.csv, |row| row.some_key == .some_field)
```

Note the ability to specify closures this is not yet available in VRL.

The advantage of this is it allows the user to specify a more complex
set of criteria beyond simple equality. The downside is that it would be very
hard use indexes to ensure the lookup remains performant.

There is nothing that would prevent us from providing both options.

### Specify the file directly in VRL without using EnrichmentTable

Instead of using a separate section to specify the enrichment table, we could
require the filename te be specified within VRL.

```coffee
find_table_row("/path/to/file.csv", criteria)
```

This reduces the complexity for the configuration and makes the initial change
simpler since the entire functionality can be implemented by providing an
additional VRL function.

However, we lose the advantages of allowing the Admin to keep the table
separate from the main configuration. Future changes to allow different table
types, enable encryption and reloading also become more complicated.

### Provide other data sources

This RFC proposes using a CSV file as a datasource. In time we may also need to
source the data from a JSON file, SQL database or Http source.

Using SQL and Http in such a way that would require VRL to perform a lookup
each event would significantly impact performance and would also require a
fairly significant modification to VRL to allow VRL functions to run
asynchronously.

## Outstanding Questions

## Plan Of Attack

- [ ] Add support for enrichment tables. Any table sections will load the data
      at boot time and will prevent Vector from starting up if the source file
      cannot be found or is incorrectly formatted.
- [ ] Wire up the topology so Transforms have access to the enrichment tables.
- [ ] Update VRL to allow the Remap Transform and Conditions to pass any tables
      into the program. VRL needs the tables at compile time to ensure the named
      table is available and at run time to access the data.
- [ ] Implement `find_table_row` VRL function.
