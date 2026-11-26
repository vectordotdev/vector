//! OTLP-derived Datadog `Span.type` and `Span.resource`.
//!
//! `Span.type` follows the Datadog Agent `SpanKind2Type` (`pkg/trace/transform/otelutil.go`):
//! resource `db.system` is consulted before the span attribute. `Span.resource` follows
//! current `GetOTelResourceV2`: each concept key is read from the span, then the resource,
//! and only a non-empty string counts. Derived names are truncated to `MaxResourceLen`
//! on a UTF-8 boundary.

use std::borrow::Cow;

use crate::event::typed_trace::{AttrValue, Attributes, Resource, Span, SpanKind};

/// Datadog Agent `normalize.MaxResourceLen`.
const MAX_RESOURCE_LEN: usize = 5000;

/// Derives wire `Span.type` when [`crate::event::typed_trace::DatadogSpanContext::span_type`]
/// is absent. Every result is a static Datadog type name.
#[must_use]
pub(super) fn derive_span_type(span: &Span, resource: &Resource) -> &'static str {
    match span.kind {
        SpanKind::Server => "web",
        SpanKind::Client => match db_system(span, resource) {
            Some(db) if is_cache_system(db) => "cache",
            Some(_) => "db",
            None => "http",
        },
        SpanKind::Internal
        | SpanKind::Producer
        | SpanKind::Consumer
        | SpanKind::Unspecified
        | SpanKind::Other(_) => "custom",
    }
}

/// Derives wire `Span.resource` when
/// [`crate::event::typed_trace::DatadogSpanContext::resource_name`] is absent.
///
/// Borrows an attribute or `Span.name` when that text is the result. Owns a
/// string only when two parts are joined.
#[must_use]
pub(super) fn derive_resource_name<'a>(span: &'a Span, resource: &'a Resource) -> Cow<'a, str> {
    truncate_utf8(resource_name(span, resource), MAX_RESOURCE_LEN)
}

fn db_system<'a>(span: &'a Span, resource: &'a Resource) -> Option<&'a str> {
    // `SpanKind2Type` uses `AsString`, which is empty for a non-string, so that
    // value is absent and the other scope can still supply `db.system`.
    non_empty_string(&resource.attributes, "db.system")
        .or_else(|| non_empty_string(&span.attributes, "db.system"))
}

const fn is_cache_system(db: &str) -> bool {
    db.eq_ignore_ascii_case("redis") || db.eq_ignore_ascii_case("memcached")
}

fn resource_name<'a>(span: &'a Span, resource: &'a Resource) -> Cow<'a, str> {
    if let Some(name) = text_at(span, resource, &["resource.name"]) {
        Cow::Borrowed(name)
    } else if let Some(method) = http_method(span, resource) {
        http_resource(span, resource, method)
    } else if let Some(operation) = text_at(span, resource, &["messaging.operation"]) {
        join_optional(
            Cow::Borrowed(operation),
            text_at(
                span,
                resource,
                &["messaging.destination", "messaging.destination.name"],
            ),
        )
    } else if let Some(method) = text_at(span, resource, &["rpc.method"]) {
        join_optional(
            Cow::Borrowed(method),
            text_at(span, resource, &["rpc.service"]),
        )
    } else if let Some(operation) = text_at(span, resource, &["graphql.operation.type"]) {
        join_optional(
            Cow::Borrowed(operation),
            text_at(span, resource, &["graphql.operation.name"]),
        )
    } else if text_at(span, resource, &["db.system.name", "db.system"]).is_some()
        && let Some(statement) = text_at(span, resource, &["db.statement", "db.query.text"])
    {
        Cow::Borrowed(statement)
    } else {
        Cow::Borrowed(&span.name)
    }
}

fn http_method<'a>(span: &'a Span, resource: &'a Resource) -> Option<Cow<'a, str>> {
    text_at(span, resource, &["http.request.method", "http.method"]).map(|method| {
        if method == "_OTHER" {
            Cow::Borrowed("HTTP")
        } else {
            Cow::Borrowed(method)
        }
    })
}

fn http_resource<'a>(span: &'a Span, resource: &'a Resource, method: Cow<'a, str>) -> Cow<'a, str> {
    let suffix = match span.kind {
        SpanKind::Client => text_at(span, resource, &["url.template"]),
        SpanKind::Server => text_at(span, resource, &["http.route"]),
        SpanKind::Internal
        | SpanKind::Producer
        | SpanKind::Consumer
        | SpanKind::Unspecified
        | SpanKind::Other(_) => None,
    };
    join_optional(method, suffix)
}

fn join_optional<'a>(primary: Cow<'a, str>, extra: Option<&str>) -> Cow<'a, str> {
    match extra {
        Some(extra) => Cow::Owned(format!("{primary} {extra}")),
        None => primary,
    }
}

/// First non-empty string. Each key is taken from the span, then the resource.
///
/// This is the Agent `OTelSpanAccessor` walk used by `GetOTelResourceV2`: `GetString`
/// ignores non-strings, and an earlier concept key on the resource wins over a later
/// key that exists only on the span.
fn text_at<'a>(span: &'a Span, resource: &'a Resource, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        non_empty_string(&span.attributes, key)
            .or_else(|| non_empty_string(&resource.attributes, key))
    })
}

fn non_empty_string<'a>(attributes: &'a Attributes, key: &str) -> Option<&'a str> {
    match attributes.get(key) {
        Some(AttrValue::String(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn truncate_utf8(value: Cow<'_, str>, max_bytes: usize) -> Cow<'_, str> {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    match value {
        Cow::Borrowed(text) => Cow::Borrowed(text.get(..end).expect("end is a char boundary")),
        Cow::Owned(mut text) => {
            text.truncate(end);
            Cow::Owned(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{derive_resource_name, derive_span_type};
    use crate::event::typed_trace::{AttrValue, Resource, Span, SpanId, SpanKind};

    fn span(name: &str) -> Span {
        Span::new(SpanId::new(1).expect("non-zero"), name)
    }

    fn kind_type(kind: SpanKind, db: Option<&str>) -> &'static str {
        let mut span = span("op");
        span.kind = kind;
        if let Some(db) = db {
            span.attributes
                .insert("db.system", AttrValue::String(db.into()));
        }
        derive_span_type(&span, &Resource::default())
    }

    #[test]
    fn span_kind_to_type_follows_the_agent() {
        assert_eq!(kind_type(SpanKind::Server, None), "web");
        assert_eq!(kind_type(SpanKind::Server, Some("postgres")), "web");
        assert_eq!(kind_type(SpanKind::Client, None), "http");
        assert_eq!(kind_type(SpanKind::Client, Some("postgres")), "db");
        assert_eq!(kind_type(SpanKind::Client, Some("redis")), "cache");
        assert_eq!(kind_type(SpanKind::Client, Some("memcached")), "cache");
        assert_eq!(kind_type(SpanKind::Client, Some("Redis")), "cache");
        assert_eq!(kind_type(SpanKind::Internal, None), "custom");
        assert_eq!(kind_type(SpanKind::Producer, None), "custom");
        assert_eq!(kind_type(SpanKind::Consumer, None), "custom");
        assert_eq!(kind_type(SpanKind::Unspecified, None), "custom");
        assert_eq!(kind_type(SpanKind::from_i32(9), None), "custom");
    }

    #[test]
    fn db_system_on_the_resource_wins_over_the_span_for_type() {
        let mut span = span("op");
        span.kind = SpanKind::Client;
        span.attributes
            .insert("db.system", AttrValue::String("postgres".into()));
        let mut resource = Resource::default();
        resource
            .attributes
            .insert("db.system", AttrValue::String("redis".into()));
        assert_eq!(derive_span_type(&span, &resource), "cache");

        resource
            .attributes
            .insert("db.system", AttrValue::Integer(1));
        span.attributes
            .insert("db.system", AttrValue::String("postgres".into()));
        assert_eq!(derive_span_type(&span, &resource), "db");

        span.attributes.insert("db.system", AttrValue::Integer(1));
        assert_eq!(derive_span_type(&span, &resource), "http");
    }

    #[test]
    fn resource_name_uses_span_attributes_then_name() {
        let mut span = span("fallback");
        span.kind = SpanKind::Server;
        span.attributes
            .insert("http.request.method", AttrValue::String("GET".into()));
        span.attributes
            .insert("http.route", AttrValue::String("/users".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "GET /users"
        );

        span.kind = SpanKind::Client;
        span.attributes
            .insert("url.template", AttrValue::String("/users/{id}".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "GET /users/{id}"
        );

        span.attributes
            .insert("http.request.method", AttrValue::String("_OTHER".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "HTTP /users/{id}"
        );
    }

    #[test]
    fn resource_name_covers_rpc_messaging_graphql_and_db() {
        let mut span = span("fallback");
        span.attributes
            .insert("rpc.method", AttrValue::String("Get".into()));
        span.attributes
            .insert("rpc.service", AttrValue::String("Users".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "Get Users"
        );

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("messaging.operation", AttrValue::String("publish".into()));
        span.attributes.insert(
            "messaging.destination.name",
            AttrValue::String("orders".into()),
        );
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "publish orders"
        );

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("graphql.operation.type", AttrValue::String("query".into()));
        span.attributes
            .insert("graphql.operation.name", AttrValue::String("User".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "query User"
        );

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("db.system", AttrValue::String("postgresql".into()));
        span.attributes
            .insert("db.query.text", AttrValue::String("select 1".into()));
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "select 1"
        );

        span.attributes.remove("db.query.text");
        assert_eq!(
            derive_resource_name(&span, &Resource::default()),
            "fallback"
        );
    }

    #[test]
    fn span_attribute_wins_over_resource_and_long_names_truncate() {
        let mut span = span("fallback");
        span.kind = SpanKind::Server;
        span.attributes
            .insert("http.route", AttrValue::String("/span".into()));
        let mut resource = Resource::default();
        resource
            .attributes
            .insert("http.route", AttrValue::String("/resource".into()));
        resource
            .attributes
            .insert("http.request.method", AttrValue::String("POST".into()));
        assert_eq!(derive_resource_name(&span, &resource), "POST /span");

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("http.method", AttrValue::String("GET".into()));
        resource.attributes = crate::event::typed_trace::Attributes::new();
        resource
            .attributes
            .insert("http.request.method", AttrValue::String("POST".into()));
        assert_eq!(derive_resource_name(&span, &resource), "POST");

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("db.query.text", AttrValue::String("select span".into()));
        resource.attributes = crate::event::typed_trace::Attributes::new();
        resource
            .attributes
            .insert("db.system.name", AttrValue::String("postgresql".into()));
        resource
            .attributes
            .insert("db.statement", AttrValue::String("select resource".into()));
        assert_eq!(derive_resource_name(&span, &resource), "select resource");

        span.attributes = crate::event::typed_trace::Attributes::new();
        span.attributes
            .insert("http.request.method", AttrValue::Integer(1));
        span.attributes
            .insert("http.method", AttrValue::String("GET".into()));
        assert_eq!(derive_resource_name(&span, &Resource::default()), "GET");

        let mut name = "a".repeat(4999);
        name.push('é');
        name.push('z');
        assert_eq!(
            derive_resource_name(
                &Span::new(SpanId::new(2).expect("non-zero"), &name),
                &Resource::default()
            ),
            "a".repeat(4999)
        );
        assert_eq!(
            derive_resource_name(
                &Span::new(SpanId::new(3).expect("non-zero"), "b".repeat(5001)),
                &Resource::default()
            )
            .len(),
            5000
        );
    }
}
