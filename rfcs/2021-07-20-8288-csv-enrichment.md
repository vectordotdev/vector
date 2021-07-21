# RFC 8288 - 2021-07-20 - CSV Enrichment

Vector needs to allow users to enrich the events flowing through the topology
using through the topology using a CSV file. This RFC proposes adding a Vrl
function that can perform this lookup.

## Scope

The RFC will cover a simple VRL function that will lookup a single row from a
CSV file. 

The lookup will be a simple sequential scan. The first version of this
function will not handle any indexing of this data. Above a certain size
indexing will provide substantial performance benefits, so it is something that
we should definitely plan for implementing at a future stage. The RFC will 
briefly discuss this, but will not go into any specific indexing strategies.

## Motivation

Users would like to enrich events flowing through Vector with extra data 
provided by a CSV file. 

## Doc-level Proposal

Two remap functions:

### `lookup_csv_row`

This function will look up a single row within the dataset. If a single row is 
found that data is returned as an object, otherwise this function will error. 

### `lookup_csv_rows` 

This function returns the results as an array of objects. If no row is found an
empty array is returned. Multiple results just result in multiple rows in the 
array.

### Parameters 

*filename*

The filename is the path to the filename of the csv file to lookup. This
filename has to be a static string (cannot be changed at runtime) so that 
Vector can:

1. Validate that the file exists.
2. Load the file into memory.

The filename will be relative to the current working directory that Vector is
running in.

The CSV file must start with a title row that gives a name to each of the 
columns provided in the file.

*key*

The CSV file may be encrypted. This parameter specifies a key to use to decrypt
the file.

Is it safe to assume/require the file to be encrypted using AES, or should we
support other algorithms as well?

*criteria*

`criteria` is a single level, key/value object that specifies the fields and
values to lookup. The fields must all match (AND) for the row to be returned.

*case_insensitive*

By default the search will be case sensitive, but this can be changed by 
passing `true` to this parameter.

## Internal Proposal

- Describe your change as if you were presenting it to the Vector team.
- Use lists, examples, and code blocks for efficient reading.
- Be specific!

The majority of the changes will be provided by adding two new functions within
`vrl/stdlib` and some shared lookup code.

Since the CSV file is loaded at bootime, it is not essential that it is loaded 
using async. This makes things much simpler as currently the VRL compilation 
process is entirely synchronous. No code outside of the new function in the
vrl stdlib should need changing.

The entire data file will be loaded into memory, so all lookups will be 
performed in memory.

### Indexing 

Although the first version is not going to be doing any indexing into the data
it is worth bearing it in mind as we will most likely need to add this in due
course.

In order to perform the indexing VRL needs to know which fields to index. The 
criteria is being passed in as an object. If the type def for that object is
known at runtime, we can extract the fields from this. 

If the type def isn't known we could 

- raise an error 
- perform the search unindexed
- create the index at runtime - generally the shape of this object will not 
  change over time, so most likely the cost of creating the index would only
  need to be paid on the first event.

The approach we take here should be decided before the first release so we don't
have to introduce a breaking change further down the line.

Actual indexing strategies can be decided later.

## Rationale

There is significant customer demand for this feature.

## Drawbacks

1. There are a number of issues with parsing CSV files in particular around
   handling delimeters and separators.
2. Performance. Whilst we can ensure all the IO is performed at boot time,
   searching through that data could still be quite expensive.
3. Memory use. Since the data file is loaded into memory, the data will use up 
   some memory. If the file is large enough that could have an impact should
   Vector be running in a constrained environment.

## Alternatives

### Use a predicate for the search

Instead of using an object to specify the search criteria we could allow the
user to specify a predicate to determine the row to use for enrichment.

```
lookup_csv_row("/path/to/file.csv", |row| row.some_key = .some_field)
```

Note the ability to specify closures this is not yet available in VRL.

The advantage of this is it allows the user to specify a more complex
set of criteria beyond simple equality. The downside is that it would be very
hard use indexes to ensure the lookup remains performant.

There is nothing that would prevent us from providing both options.

### Provide other data sources

This RFC proposes using a CSV file as a datasource. It time we may also need to
source the data from a JSON file, SQL database or Http source.

Using SQL and Http in such a way that would require VRL to perform a lookup
each event would significantly impact performance and would also require a 
fairly significant modification to VRL to allow VRL functions to run 
asynchronously.

## Outstanding Questions

- What approach should we taken around determining the indexed fields?
- Can we assume that if the file is encrypted it will use AES?

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.
