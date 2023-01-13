# Log Namespacing

This walks through the steps required to add log namespacing to a given source.

Log Namespacing as a new feature in Vector that allows different fields of the Log 
event to be kept under separate namespaces, thus avoiding conflicts where two different
fields try to have the same name. Log Namespacing does not apply to Metric events.

## Config

Add the following field to the `Config` struct:

```rust
    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
```

Currently, because log namespacing is an unreleased feature we add the `docs::hidden`
metadata so it doesn't appear in the documentation.

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
when using the Legacy Namespace. This will typically be values returned
by calls to `log_schema().source_type_key()` and `log_schema().timestamp_key()`.

#### metdata_key

The name of the field when it is inserted into the Vector namespace. This 
will be `path!("ingest_timestamp")` or `path!(source_type)`. The field names
can be hardcoded since they are going into the Vector namespace, so conflicts
with other field names cannot occur.

#### value

The actual value to be placed into the field. 

For the ingest timestamp this will be `chrono::Utc::now()`. Source type will be
the `NAME` property of the `Config` struct. `NAME` is provided by the 
`configurable_component` macro. You may need to include `use vector_config::NamedComponent;`.

### insert_standard_vector_source_metadata

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
should go into the source metadata. Examples of source metadata are 

- The Kafka topic when pulling from a Kafka stream.
- Severity and Facility fields from a Syslog message.
- The file path when pulling data from a file.
- The timestamp extracted from the message (this can be different to the time
  the message was ingested.)

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
of different things.

- Ingest timestamp - This is the timestamp when the event was received
  by Vector. This should go in the Vector metadata.
- Timestamp - This should be any timestamp extracted from the incoming 
  message. No timestamp 
  
It is worth noticing that existing sources have not always been consistent
with this. Some sources would insert a timestamp that is extracted from the
event but default to the ingest timestamp if it didn't exist. Others insert 
the timestamp extracted from the event and don't insert a timestamp at all
if it didn't exist. Others will always insert the ingest timestamp. To 
maintain backward compatibility there is a few areas in the code base that
do some seemingly overly complicated things with timestamps. It is worth 
bearing this in mind when looking through existing new code.

All new sources should work like the above.
