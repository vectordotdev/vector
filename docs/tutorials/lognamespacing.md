# Log Namespacing

This walks through the steps required to add log namespacing to a given source.

Log Namespacing is a new feature in Vector that allows different fields of the Log
event to be kept under separate namespaces, thus avoiding conflicts where two different
fields try to use the same name. Log Namespacing does not apply to Metric or Trace events.

## Config

Add the following field to the `Config` struct:

```rust
    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
```

Currently, because log namespacing is an unreleased feature we add the `docs::hidden`
attribute so it doesn't appear in the documentation.

## Build

The configuration is currently just a bool, switching it on or off. When we come to
retrieve the actual namespace to use we merge it in with the globally configured one.
This is passed in via the `SourceContext` parameter.

```rust
impl SourceConfig for DnstapConfig {
    async fn build(&self, cx: SourceContext) -> Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
```

The `cx.log_namespace` function gives us a `LogNamespace` enum that we can use to
set the fields in the appropriate section of the Event.

This `log_namespace` variable needs to be passed to any functions that will insert
data into the log event that is emitted by the source.

### Vector metadata

The Vector namespace contains data pertinent to how the event was ingested into
Vector. Currently two fields need to be added to this namespace - `ingest_timestamp`
and `source_type`:

```rust
  self.log_namespace.insert_vector_metadata(
      &mut log_event,
      path!(self.timestamp_key()),
      path!("ingest_timestamp"),
      chrono::Utc::now(),
  );

  self.log_namespace.insert_vector_metadata(
      &mut log_event,
      path!(self.source_type_key()),
      path!("source_type"),
      DnstapConfig::NAME,
  );
```

If we look at the parameters to `insert_vector_metadata`:

```rust
    pub fn insert_vector_metadata<'a>(
        &self,
        log: &mut LogEvent,
        legacy_key: impl ValuePath<'a>,
        metadata_key: impl ValuePath<'a>,
        value: impl Into<Value>,
    )
```

#### log

This needs to be the log event that is being populated.

#### legacy_key

This is the name of the field the timestamp is to be inserted into
when using the Legacy Namespace.

The value for this field comes from a number of different places.

- For fields that are typically found in most log events the value will
be returned by calls to `log_schema()` eg. `log_schema().source_type_key()`
or `log_schema().timestamp_key()`.
- Some sources allow the user to specify the field name that a given
value will be placed in. For example, the `kafka` source will allow the
user to specify the `topic_key` - the field name that will contain the
kafka `topic` the event was consumed from.
- Other sources just hard code this value. For example the `dnstap` source
creates an event with an object where most of the field names are hard coded.

#### metadata_key

The name of the field when it is inserted into the Vector namespace. This
will be `path!("ingest_timestamp")` or `path!("source_type")`. The field names
can be hard coded since they are going into the Vector namespace, so conflicts
with other field names cannot occur.

It should be noted that the values for these field names are typically
hard coded. With the `kafka` source, for example, it was possible to configure
the field name that the `topic` was inserted into. In the Vector namespace
this field name is just hard coded to `topic`. Allowing the user to configure
the fieldname was only necessary to prevent name conflicts with other values
from the event. This is no longer an issue as these values are now placed in a
separate namespace to the event data.

#### value

The actual value to be placed into the field.

For the ingest timestamp this will be `chrono::Utc::now()`. Source type will be
the `NAME` property of the `Config` struct. `NAME` is provided by the
`configurable_component` macro. You may need to include `use vector_config::NamedComponent;`.

For batches of events, each event in the batch should use a precalculated
`Utc::now()` so they all share the same timestamp.


### insert_standard_vector_source_metadata(...)

A utility function has been provided that can be used in a lot of cases to
insert both these fields into the Vector namespace:

```rust

  log_namespace.insert_standard_vector_source_metadata(
      log,
      KafkaSourceConfig::NAME,
      Utc::now(),
  );
```

### Source Metadata

Other fields that describe the event - but are not the actual data for the event
should go into the source metadata. Examples of source metadata are:

- The Kafka topic when pulling from a Kafka stream.
- Severity and Facility fields from a Syslog message.
- The file path when pulling data from a file.

To insert source metadata:

```rust
  log_namespace.insert_source_metadata(
      SyslogConfig::NAME,
      log,
      Some(LegacyKey::Overwrite("source_id")),
      path!("source_id"),
      default_host.clone(),
  );
```

Let's look at the parameters:

```rust
    pub fn insert_source_metadata<'a>(
        &self,
        source_name: &'a str,
        log: &mut LogEvent,
        legacy_key: Option<LegacyKey<impl ValuePath<'a>>>,
        metadata_key: impl ValuePath<'a>,
        value: impl Into<Value>,
    )
```

#### source_name

The name of the source. This will be eg. `KafkaSourceConfig::NAME`.

#### log

The log event to populate.

#### legacy_key

The field name to populate for the legacy namespace. Pass `None` if
this field should not be inserted for Legacy. Because there is a
possibility that the field might conflict with another field that
is already in the event what to do in the case of conflicts must
also be specified. `LegacyKey::Overwrite` will overwrite the existing
value with this value. `LegacyKey::InsertIfEmpty` keeps the original
value.

#### metadata_key

The name of the path to insert into the Source metadata when in
the Vector namespace. Because there is no chance of conflicting names
here, this is typically just a hard coded value. eg. `path!("topic")`

#### value

The actual value that is to be inserted into the metadata.

## The event

The main log event should contain only the real log message that the
event is representing.

For the Vector namespace the data should be at the top level and not
contained in any subfields. For an event that is a single String value -
typically, in the Legacy namespace this will be inserted in a field
called `message`. In the Vector namespace the event will be just this
String value.

In this case code that creates an event typically looks similar to:

```rust
    let mut log = match log_namespace {
        LogNamespace::Vector => LogEvent::from(message),
        LogNamespace::Legacy => {
            let mut log = LogEvent::default();

            // Add message
            log.insert(log_schema().message_key(), message);
            log
        }
    };
```

Other fields should be inserted into the event like:

```rust
  log_event.insert(event_path!("path"), value);
```

## Timestamps

We need to talk about timestamps. A timestamp can represent a number
of different things:

- Ingest timestamp - This is the timestamp when the event was received
  by Vector. This should go in the Vector metadata.
- Timestamp - This should be any timestamp extracted from the incoming
  message.

It is worth recognising that existing sources have not always been consistent
with this. Some sources would insert a timestamp that is extracted from the
event but default to the ingest timestamp if it didn't exist. Others insert
the timestamp extracted from the event and don't insert a timestamp at all
if it didn't exist. Others will always insert the ingest timestamp. To
maintain backward compatibility there is a few areas in the code base that
do some seemingly overly complicated things with timestamps. It is worth
bearing this in mind when looking through existing new code.

All new sources should work like the above and should not permit users to
configure custom field names for metadata.

## Schema

All sources need to specify their schema - a definition of the shape of the
event that it will create.

The schema definition is returned from the `outputs` function defined
by the `SourceConfig` trait.

```rust
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
```

Most sources have a decoder option that will specify the initial schema. One
can retrieve the schema by calling:

```rust
  let schema_definition = self
      .decoding
      .schema_definition(log_namespace)
```

We need to add the metadata that has been adding to the Vector namespace:

```rust
      .with_standard_vector_source_metadata()
```

Next we need to add any source metadata that is created by the source.

```rust
      .with_source_metadata(
          NatsSourceConfig::NAME,
          legacy_subject_key_field,
          &owned_value_path!("subject"),
          Kind::bytes(),
          None,
      );
```

Let's look at the parameters:

```rust
    pub fn with_source_metadata(
        self,
        source_name: &str,
        legacy_path: Option<LegacyKey<OwnedValuePath>>,
        vector_path: &OwnedValuePath,
        kind: Kind,
        meaning: Option<&str>,
    ) -> Self
```

### source_name

The name of the source - typically something like `NatsSourceConfig::NAME`

### legacy_path

The pathname of the field when inserting in the Legacy namespace. This should be the
same value as used when inserting the data with `insert_source_metadata`.

### vector_path

The pathname of the field when inserting in the Vector namespace. This should be the
same value as used when inserting the data with `insert_source_metadata`.

### kind

This is the type the data will be. This is covered in detail below.

### meaning

Some fields are given a meaning. It is possible in VRL to refer to a field by it's
meaning regardless of what name has been given to it. Fields with the following meaning
are used in Vector:

- message
- timestamp
- severity
- host
- service
- source
- tags

This list is not definitive and likely to be updated over time.

Most fields will not have a given meaning, in which case just pass `None`.


### Kind

The core principle behind schemas is defining the type, or kind, of data that will
exist in this field. The following kinds are supported:

#### bytes

Any string value.

#### integer

An integer value - in Vector this will be a signed 64 bit integer.

#### float

A 64 bit float value.

#### boolean

Boolean value - either `true` or `false`.


#### timestamp

A timestamp in the UTC timezone.

#### array

An array of values. It is possible to specify the type for any element
within the array eg. this array will be an array of strings.

```rust
Kind::array(Collection::empty().with_unknown(Kind::bytes()))
````

It is also possible to specify the type for specific indexes in the
array eg. this array will have a string at index 0 and an integer
at index 1:

```rust
Kind::array(Collection::empty()
                .with_known(0, Kind::bytes())
                .with_known(1, Kind::integer()))
```

These can also be combined. For example an array of strings apart
from the third index, which will be a timestamp:

```rust
Kind::array(Collection::empty().with_unknown(Kind::bytes())
                .with_known(3, Kind::timestamp()))
````

#### object

An object is a map of keys to values. Similar to an array, an object
can specify the type for all fields as well as the type for specific
fields.

An object where all fields will be strings, but doesn't specify what
those field names are:

```rust
Kind::object(Collection::empty().with_unknown(Kind::bytes()))
````

An object with two fields - `reason` containing a string and `value`
containing an integer:

```rust
Kind::object(Collection::empty()
                .with_known("reason", Kind::bytes())
                .with_known("value", Kind::integer()))
```


#### Multiple types

It is possible to represent a field that could be one of several types.

For example, a string or an integer:

```rust
Kind::bytes().or_integer()
```

Often a field may not exist at all, for that we have `or_undefined()`.
For example, an object with a field called `reason` that may not exist,
but if it does it will be a string:

```rust
Kind::object(Collection::empty()
              .with_known("reason", Kind::bytes().or_undefined()))
```
