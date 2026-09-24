# RFC 17195 - 2024-05-14 - External/Remote stores for enriching events

Vector needs to support enriching events from external data stores. This RFC proposes extending the Enrichment Table spec to allow writes and use the spec to configure remote stores.

## Context

- [#17195](https://github.com/vectordotdev/vector/issues/17195)

## Cross cutting concerns

- N/A

## Scope

### In scope

- Adding new VRL functions to allow point data upsert's.
- Add an enum in Enrichment Tables to mark them as read or read/write which can be used for compile-time validation of VRL.

### Out of scope

- Caching for network calls
- Indexing for write invocations

## Pain

- A significant part of data transformation relies on dynamic values which is not possible right now.
- Users need to maintain & deploy separate systems when they wanna combine data from different data stores.

## Proposal

### User Experience

- The user would be able to add more enrichment table types for their store and use them in VRL
```toml
[enrichment_tables.my_store]
  type = "redis/cassandra"
  host = "localhost" # These fields are specific to the enrichment table type
  port = "6379" # These fields are specific to the enrichment table type
  ....
```
- The user can set/update data with a VRL function providing the data and a key for his store. Currently the function expects a key to be used for KV stores.
```js
set_enrichment_table_record(table: <string>, record: <object>, key: <string>, [condition: <object>])
:: <object> , <error>
```

### Implementation

- Add a new trait to support read/write entries
```rust
pub trait MutableTable: Table {
    fn update_table_row<'a>(
        &self,
        condition: &'a [Condition<'a>],
        key: &'a String,
        value: &'a ObjectMap,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, String>;
}
```

- Update TableMap to hold an enum instead which will be returned by the `EnrichmentTableConfig::build` function.
```rust
type TableMap = HashMap<String, EnrichmentTable>;

enum EnrichmentTable {
    ReadOnly(Box<dyn Table + Send + Sync>),
    ReadWrite(Box<dyn MutableTable + Send + Sync>),
}
```

- The rest of the flow around deserializing configuration & creating clients remains synonymous with the enrichment table.

## Rationale

- This enables vector to handle a lot more use-cases around data transformations
- Vector already supports almost all sink/sources and this would solve a lot of transformation use cases eliminating the need for custom tools in the log/event pipeline

## Drawbacks

- This can slow down the vector system if it relies on external network calls
- This makes vector vulnerable to another point of failure in external stores

## Alternatives

- This can be supported via Lua transforms but there seems to be sufficient demand to provide native support for this.

## Outstanding Questions

- How do we handle schema validation for structured stores? Do we leave it to the individual implementors or we don't do any compile time validation?
- Should we have separate traits/VRL functions to enable SQL & KV stores? (Since SQL might need schema validation whereas KV needs presence of a key)

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Add a trait to represent mutable table
- [ ] Add VRL functions and handle compilation for them
- [ ] Add a mutable store for vector which implements the above traits

Note: This can be filled out during the review process.

## Future Improvements

- Add caching for remote stores
- Add compile time schema validation for SQL stores
