use chrono::{Offset, TimeZone, Utc};
use chrono_tz::Tz;
use rstest::rstest;
use vector_lib::{
    config::LogNamespace,
    lookup::{PathPrefix, metadata_path},
    metric_tags,
};
use vrl::event_path;

use super::*;
use crate::event::{Event, LogEvent, MetricKind, MetricValue};

#[test]
fn uri_template_serde_roundtrip_string() {
    // UriTemplate must serialize/deserialize as a plain string, not an object.
    let uri = UriTemplate::try_from("https://example.com/{{ tenant }}/path").unwrap();

    // Serialize to JSON string (exact scalar, not object)
    let json = serde_json::to_string(&uri).unwrap();
    assert_eq!(json, "\"https://example.com/{{ tenant }}/path\"");

    // Deserialize back from JSON string
    let uri2: UriTemplate = serde_json::from_str(&json).unwrap();
    assert_eq!(uri, uri2);
}

#[test]
fn uri_template_display_preserves_source() {
    // Display must return the original source string
    let src = "https://example.com/{{ path }}";
    let uri = UriTemplate::try_from(src).unwrap();
    assert_eq!(uri.to_string(), src);
}

#[test]
fn get_fields() {
    let f1 = UnconfinedTemplate::try_from("{{ foo }}")
        .unwrap()
        .get_fields()
        .unwrap();
    let f2 = UnconfinedTemplate::try_from("{{ foo }}-{{ bar }}")
        .unwrap()
        .get_fields()
        .unwrap();
    let f3 = UnconfinedTemplate::try_from("nofield")
        .unwrap()
        .get_fields();
    let f4 = UnconfinedTemplate::try_from("%F").unwrap().get_fields();
    let f5 = UnsignedIntTemplate::try_from("{{ foo }}-{{ bar }}")
        .unwrap()
        .get_fields()
        .unwrap();
    let f6 = UnsignedIntTemplate::from(123u64).get_fields();
    let f7 = UnsignedIntTemplate::try_from("%s").unwrap().get_fields();

    assert_eq!(f1, vec!["foo"]);
    assert_eq!(f2, vec!["foo", "bar"]);
    assert_eq!(f3, None);
    assert_eq!(f4, None);
    assert_eq!(f5, vec!["foo", "bar"]);
    assert_eq!(f6, None);
    assert_eq!(f7, None);
}

#[test]
fn literal_prefix() {
    let cases = [
        ("/var/log/app.log", "/var/log/app.log"),
        ("/var/log/{{ host }}/app.log", "/var/log/"),
        ("/var/log/%Y/{{ host }}.log", "/var/log/"),
        ("/srv-{{ id }}.log", "/srv-"),
        ("{{ full_path }}", ""),
        ("/{{ tenant }}/app.log", "/"),
        // `%%` stops the prefix scan — in mixed segments like
        // `100%%/%Y/{{ x }}` chrono decodes `%%` while expanding `%Y`,
        // so we cannot determine the rendered prefix without a timestamp.
        ("100%%-literal/{{ x }}", "100"),
        ("no-template-at-all", "no-template-at-all"),
        ("only-strftime-%F.log", "only-strftime-"),
        // single `{` is not a field opener
        ("a{b/{{ c }}", "a{b/"),
    ];
    for (src, expected) in cases {
        let tpl = UnconfinedTemplate::try_from(src).unwrap();
        assert_eq!(tpl.literal_prefix(), expected, "src = {src:?}");
    }
}

#[test]
fn is_dynamic() {
    assert!(
        UnconfinedTemplate::try_from("/kube-demo/%F")
            .unwrap()
            .is_dynamic()
    );
    assert!(
        !UnconfinedTemplate::try_from("/kube-demo/echo")
            .unwrap()
            .is_dynamic()
    );
    assert!(
        UnconfinedTemplate::try_from("/kube-demo/{{ foo }}")
            .unwrap()
            .is_dynamic()
    );
    assert!(
        UnconfinedTemplate::try_from("/kube-demo/{{ foo }}/%F")
            .unwrap()
            .is_dynamic()
    );
}

#[test]
fn render_log_static() {
    let event = Event::Log(LogEvent::from("hello world"));
    let template = UnconfinedTemplate::try_from("foo").unwrap();

    assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
}

#[test]
fn render_log_unsigned_number() {
    let event = Event::Log(LogEvent::from("hello world"));
    let template = UnsignedIntTemplate::from(123);

    assert_eq!(Ok(123), template.render(&event))
}

#[test]
fn render_log_unsigned_number_dynamic() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("foo"), 123);

    let template = UnsignedIntTemplate::try_from("{{ foo }}").unwrap();
    assert_eq!(Ok(123), template.render(&event))
}

#[test]
fn render_log_dynamic() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(event_path!("log_stream"), "stream");
    let template = UnconfinedTemplate::try_from("{{log_stream}}").unwrap();

    assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
}

#[test]
fn render_log_metadata() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(metadata_path!("metadata_key"), "metadata_value");
    let template = UnconfinedTemplate::try_from("{{%metadata_key}}").unwrap();

    assert_eq!(Ok(Bytes::from("metadata_value")), template.render(&event))
}

#[test]
fn render_log_dynamic_with_prefix() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(event_path!("log_stream"), "stream");
    let template = UnconfinedTemplate::try_from("abcd-{{log_stream}}").unwrap();

    assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
}

#[test]
fn render_log_dynamic_with_postfix() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(event_path!("log_stream"), "stream");
    let template = UnconfinedTemplate::try_from("{{log_stream}}-abcd").unwrap();

    assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
}

#[test]
fn render_log_dynamic_missing_key() {
    let event = Event::Log(LogEvent::from("hello world"));
    let template = UnconfinedTemplate::try_from("{{log_stream}}-{{foo}}").unwrap();

    assert_eq!(
        Err(TemplateRenderingError::MissingKeys {
            missing_keys: vec!["log_stream".to_string(), "foo".to_string()]
        }),
        template.render(&event)
    );
}

#[test]
fn render_log_dynamic_multiple_keys() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("foo"), "bar");
    event.as_mut_log().insert(event_path!("baz"), "quux");
    let template = UnconfinedTemplate::try_from("stream-{{foo}}-{{baz}}.log").unwrap();

    assert_eq!(
        Ok(Bytes::from("stream-bar-quux.log")),
        template.render(&event)
    )
}

#[test]
fn render_log_dynamic_weird_junk() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("foo"), "bar");
    event.as_mut_log().insert(event_path!("baz"), "quux");
    let template = UnconfinedTemplate::try_from(r"{stream}{\{{}}}-{{foo}}-{{baz}}.log").unwrap();

    assert_eq!(
        Ok(Bytes::from(r"{stream}{\{{}}}-bar-quux.log")),
        template.render(&event)
    )
}

#[test]
fn render_log_timestamp_strftime_style() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

    let template = UnconfinedTemplate::try_from("abcd-%F").unwrap();

    assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
}

#[test]
fn render_log_timestamp_strftime_style_namespace() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("@timestamp"), ts);
    // use Vector namespace instead of legacy
    LogNamespace::Vector.insert_vector_metadata(
        event.as_mut_log(),
        Some(vrl::path!("foo")),
        vrl::path!("foo"),
        "bar",
    );
    let new_schema = event
        .as_mut_log()
        .metadata()
        .schema_definition()
        .as_ref()
        .clone()
        .with_meaning(parse_target_path("@timestamp").unwrap(), "timestamp");
    event
        .as_mut_log()
        .metadata_mut()
        .set_schema_definition(&std::sync::Arc::new(new_schema));

    let template = UnconfinedTemplate::try_from("abcd-%F").unwrap();

    assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
}

#[test]
fn render_log_timestamp_multiple_strftime_style() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

    let template = UnconfinedTemplate::try_from("abcd-%F_%T").unwrap();

    assert_eq!(
        Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
        template.render(&event)
    )
}

#[test]
fn render_log_dynamic_with_strftime() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("foo"), "butts");
    event.as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        ts,
    );

    let template = UnconfinedTemplate::try_from("{{ foo }}-%F_%T").unwrap();

    assert_eq!(
        Ok(Bytes::from("butts-2001-02-03_04:05:06")),
        template.render(&event)
    )
}

#[test]
fn render_log_dynamic_with_nested_strftime() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("format"), "%F");
    event.as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        ts,
    );

    let template = UnconfinedTemplate::try_from("nested {{ format }} %T").unwrap();

    assert_eq!(
        Ok(Bytes::from("nested %F 04:05:06")),
        template.render(&event)
    )
}

#[test]
fn render_log_dynamic_with_reverse_nested_strftime() {
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");

    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(&parse_target_path("\"%F\"").unwrap(), "foo");
    event.as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        ts,
    );

    let template = UnconfinedTemplate::try_from("nested {{ \"%F\" }} %T").unwrap();

    assert_eq!(
        Ok(Bytes::from("nested foo 04:05:06")),
        template.render(&event)
    )
}

#[test]
fn render_metric_timestamp() {
    let template = UnconfinedTemplate::try_from("timestamp %F %T").unwrap();

    assert_eq!(
        Ok(Bytes::from("timestamp 2002-03-04 05:06:07")),
        template.render(&sample_metric())
    );
}

#[test]
fn render_metric_with_tags() {
    let template =
        UnconfinedTemplate::try_from("name={{name}} component={{tags.component}}").unwrap();
    let metric = sample_metric().with_tags(Some(metric_tags!(
        "test" => "true",
        "component" => "template",
    )));
    assert_eq!(
        Ok(Bytes::from("name=a-counter component=template")),
        template.render(&metric)
    );
}

#[test]
fn render_metric_without_tags() {
    let template =
        UnconfinedTemplate::try_from("name={{name}} component={{tags.component}}").unwrap();
    assert_eq!(
        Err(TemplateRenderingError::MissingKeys {
            missing_keys: vec!["tags.component".into()]
        }),
        template.render(&sample_metric())
    );
}

#[test]
fn render_metric_with_namespace() {
    let template = UnconfinedTemplate::try_from("namespace={{namespace}} name={{name}}").unwrap();
    let metric = sample_metric().with_namespace(Some("vector-test"));
    assert_eq!(
        Ok(Bytes::from("namespace=vector-test name=a-counter")),
        template.render(&metric)
    );
}

#[test]
fn render_metric_without_namespace() {
    let template = UnconfinedTemplate::try_from("namespace={{namespace}} name={{name}}").unwrap();
    let metric = sample_metric();
    assert_eq!(
        Err(TemplateRenderingError::MissingKeys {
            missing_keys: vec!["namespace".into()]
        }),
        template.render(&metric)
    );
}

#[test]
fn render_log_with_timezone() {
    let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

    let template = UnconfinedTemplate::try_from("vector-%Y-%m-%d-%H.log").unwrap();
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        ts,
    );

    let tz: Tz = "Asia/Singapore".parse().unwrap();
    let offset = Some(Utc::now().with_timezone(&tz).offset().fix());
    assert_eq!(
        Ok(Bytes::from("vector-2001-02-03-12.log")),
        template.with_tz_offset(offset).render(&event)
    );
}

#[test]
fn render_log_unsigned_int_with_timezone() {
    let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

    let template = UnsignedIntTemplate::try_from("%Y%m%d%H").unwrap();
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("timestamp"), ts);

    let tz: Tz = "Asia/Singapore".parse().unwrap();
    let offset = Some(Utc::now().with_timezone(&tz).offset().fix());

    assert_eq!(
        Ok(2001020312),
        template.with_tz_offset(offset).render(&event)
    );
}

fn sample_metric() -> Metric {
    Metric::new(
        "a-counter",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.1 },
    )
    .with_timestamp(Some(
        Utc.with_ymd_and_hms(2002, 3, 4, 5, 6, 7)
            .single()
            .expect("invalid timestamp"),
    ))
}

#[test]
fn strftime_error() {
    assert_eq!(
        UnconfinedTemplate::try_from("%E").unwrap_err(),
        TemplateParseError::StrftimeError
    );
}

#[test]
fn strftime_non_int_result() {
    let template = UnsignedIntTemplate::try_from("a-%s").unwrap();
    let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("timestamp"), ts);

    assert_eq!(
        Err(TemplateRenderingError::NotNumeric {
            input: "a-981173106".to_owned()
        }),
        template.render(&event)
    );
}

#[test]
fn dotdot_bypass_rejected() {
    // `safe/../../escape` passes a naive starts_with("safe/") check but must
    // be rejected — on filesystem-like protocols (WebHDFS) the server resolves
    // `..` and the value escapes the intended namespace.
    let tpl = Template::try_from("safe/{{ tenant }}/").unwrap();
    let c = ConfinementChecker::for_prefix_template(&tpl)
        .unwrap()
        .unwrap();

    assert!(c.confine("safe/legit/").is_ok());
    assert!(matches!(
        c.confine("safe/../../escape/").unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("safe/../escape/").unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    // `..` only as a literal substring (not a full segment) is fine
    assert!(c.confine("safe/not..dotdot/").is_ok());
}

#[test]
fn uri_path_traversal_rejected() {
    // `https://api.internal/ingest/{{ tenant }}` with `../../admin` passes the
    // authority check (same host) but must be caught by the path-prefix +
    // `..` checks.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();

    assert!(c.confine("https://api.internal/ingest/acme").is_ok());
    assert!(matches!(
        c.confine("https://api.internal/ingest/../../admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    // Path that doesn't start with /ingest/ is also rejected.
    assert!(matches!(
        c.confine("https://api.internal/admin/secret").unwrap_err(),
        ConfineError::OutsideBase { .. }
    ));
}

#[test]
fn uri_percent_encoded_dotdot_rejected() {
    // Servers that decode percent-encoding before path resolution can be
    // tricked by `%2e%2e` instead of `..`.  All encoded variants must be
    // rejected alongside the literal `..`.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();

    assert!(c.confine("https://api.internal/ingest/legit").is_ok());
    assert!(matches!(
        c.confine("https://api.internal/ingest/%2e%2e/admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/%2E%2E/admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/.%2e/admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/%2e./admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
}

#[test]
fn uri_encoded_slash_traversal_rejected() {
    // `%2f` is a percent-encoded `/`. RFC 3986 treats it as a literal slash
    // character inside a segment, not a path separator — so `http::Uri` keeps
    // `%2e%2e%2fadmin` as a single segment and the segment-level dot-dot checks
    // alone would miss it. Many HTTP servers decode `%2f` before resolving the
    // path, turning the single segment into `../admin` and escaping the prefix.
    // We must also reject `%5c` (encoded backslash) for Windows-backed services.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();

    assert!(matches!(
        c.confine("https://api.internal/ingest/%2e%2e%2fadmin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/%2e%2e%2Fadmin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/safe%2fpath")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/safe%5cpath")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
}

#[test]
fn uri_authority_mismatch_rejected() {
    let tpl = Template::try_from("https://trusted.example.com/{{ path }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();

    // Normal path extension is fine.
    assert!(c.confine("https://trusted.example.com/api/v1").is_ok());

    // `@`-userinfo injection: `logs.example.com` becomes the username
    // and `evil.com` becomes the actual host.
    assert!(matches!(
        c.confine("https://trusted.example.com@evil.com/steal")
            .unwrap_err(),
        ConfineError::UriAuthorityMismatch { .. }
    ));

    // Host-extension attack: appending to the hostname routes to a
    // different host entirely.
    assert!(matches!(
        c.confine("https://trusted.example.com.evil.com/steal")
            .unwrap_err(),
        ConfineError::UriAuthorityMismatch { .. }
    ));

    // `@` in a URI path is fine — it's structurally after the authority.
    assert!(
        c.confine("https://trusted.example.com/path/%40user")
            .is_ok()
    );
    assert!(c.confine("https://trusted.example.com/path/@user").is_ok());

    // Wrong scheme is rejected.
    assert!(matches!(
        c.confine("http://trusted.example.com/api/v1").unwrap_err(),
        ConfineError::UriAuthorityMismatch { .. }
    ));
}

#[test]
fn uri_path_field_cannot_smuggle_backslash() {
    // Raw `\` is accepted by `http::Uri` but Windows/IIS servers treat it
    // as a path separator — `/ingest/..\admin` escapes the prefix on those
    // hosts. Reject at render time.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();
    assert!(matches!(
        c.confine("https://api.internal/ingest/..\\admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/foo\\..\\admin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
}

#[test]
fn uri_path_field_cannot_smuggle_double_encoded_traversal() {
    // `%25` is the encoded form of `%`. A proxy that decodes once turns
    // `%252e%252e%252fadmin` into `%2e%2e%2fadmin`; a second decoder
    // resolves it to `../admin`. Reject at render time.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();
    assert!(matches!(
        c.confine("https://api.internal/ingest/%252e%252e%252fadmin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
}

#[test]
fn uri_path_field_cannot_smuggle_query_or_fragment() {
    // Templates with no `?`/`#` build successfully. A field value that
    // smuggles `?...` or `#...` into the path (e.g. tenant=`ok?tenant=evil`)
    // is caught at render time: neither is part of the operator-authored
    // path prefix, and `http::Uri` would truncate at `#` before the checker
    // sees the full value.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_uri_template(&tpl).unwrap().unwrap();
    assert!(matches!(
        c.confine("https://api.internal/ingest/ok?tenant=evil")
            .unwrap_err(),
        ConfineError::OutsideBase { .. }
    ));
    assert!(matches!(
        c.confine("https://api.internal/ingest/ok#evil")
            .unwrap_err(),
        ConfineError::OutsideBase { .. }
    ));
}

#[test]
fn uri_fragment_prevents_suffix_hiding() {
    // Fragment must be rejected before parsing to prevent truncation from
    // hiding an operator-authored suffix. Template:
    //   https://api.internal/base/{{ tenant }}/ingest
    // with tenant = "ok#evil" renders:
    //   https://api.internal/base/ok#evil
    // Without raw fragment checking, http::Uri would truncate to:
    //   https://api.internal/base/ok
    // and the /ingest suffix is never checked or used.

    let config = ConfinementConfig::default();
    let tpl = UriTemplate::try_from("https://api.internal/base/{{ tenant }}/ingest").unwrap();
    let confined = tpl.confine(&config, "http", "uri").unwrap();

    let mut event = Event::Log(LogEvent::from("x"));
    event.as_mut_log().insert(event_path!("tenant"), "ok#evil");

    // Fragment in field value must be rejected
    assert!(
        matches!(
            confined.render_string(&event).unwrap_err(),
            TemplateRenderingError::Confined { .. }
        ),
        "Expected fragment to be rejected before parsing hides /ingest"
    );
}

#[test]
fn uri_template_with_query_or_fragment_rejected_at_build() {
    // URI templates with field references AND `?` / `#` are rejected. A
    // field-rendered value could smuggle a query segment, or a `#frag`
    // that `http::Uri` truncates before the checker can see the path
    // (silently dropping any operator-authored suffix).
    // Static URI templates (no `{{ }}`) are exempt — their query/fragment
    // is fixed.
    for template_str in &[
        "https://api.internal/ingest?tenant={{ tenant }}",
        "https://api.internal/ingest?tenant=team-{{ tenant }}",
        "https://api.internal/{{ path }}?tenant={{ tenant }}",
        "https://api.internal/ingest/{{ path }}?time=%Y",
        "https://api.internal/ingest/{{ tenant }}?source=vector",
        "https://api.internal/base/{{ tenant }}/ingest#frag",
        "https://api.internal/base/{{ tenant }}#frag",
        // A field extending a pathless authority still hits this check
        // when a query is present.
        "https://api.internal{{ path }}?date=x",
    ] {
        let tpl = Template::try_from(*template_str).unwrap();
        assert!(
            matches!(
                ConfinementChecker::for_uri_template(&tpl).unwrap_err(),
                BuildError::DynamicUriQueryOrFragment { .. }
            ),
            "expected DynamicUriQueryOrFragment for {template_str}"
        );
    }
}

#[rstest]
// A non-URI template whose first component is strftime (e.g. S3
// `key_prefix: "%Y/%m/"`, Kafka topic `%Y-%m`) has no literal prefix but
// also no event-field references. Its rendered value is operator-authored
// (time-derived), so prefix confinement must not be required.
#[case::s3_key_prefix("%Y/%m/")]
#[case::kafka_topic("%Y-%m")]
#[case::log_file_pattern("logs-%Y/%m/%d.log")]
fn strftime_only_prefix_template_needs_no_confinement(#[case] template_str: &str) {
    let tpl = Template::try_from(template_str).unwrap();
    assert!(
        ConfinementChecker::for_prefix_template(&tpl)
            .unwrap()
            .is_none(),
        "expected no prefix confinement for {template_str}"
    );
}

#[test]
fn prefix_template_with_strftime_confinement() {
    // When the template also references event fields, confinement still
    // applies to the literal prefix.
    let tpl = Template::try_from("logs-%Y/{{ tenant }}/").unwrap();
    let checker = ConfinementChecker::for_prefix_template(&tpl)
        .unwrap()
        .unwrap();
    assert!(checker.confine("logs-2001/tenant-a/").is_ok());
    assert!(checker.confine("other/tenant-a/").is_err());

    // A template whose FIRST component is strftime AND that references
    // event fields has no literal prefix to confine to while still carrying
    // event-controlled content — rejected (NoDerivableBase) without the
    // opt-out.
    let tpl = Template::try_from("%Y/{{ tenant }}/").unwrap();
    assert!(matches!(
        ConfinementChecker::for_prefix_template(&tpl).unwrap_err(),
        BuildError::NoDerivableBase { .. }
    ));
}

#[test]
fn no_static_uri_authority_rejected() {
    // `https://{{ host }}/ingest` has prefix `https://` — any host can be
    // rendered, so the confinement is meaningless. Must be rejected at build.
    for template_str in &["https://{{ host }}/ingest", "http://{{ host }}/path"] {
        let tpl = Template::try_from(*template_str).unwrap();
        assert!(
            matches!(
                ConfinementChecker::for_uri_template(&tpl).unwrap_err(),
                BuildError::NoStaticUriAuthority { .. }
            ),
            "expected NoStaticUriAuthority for {template_str}"
        );
    }
    // A template with a static host followed by a `/` is accepted.
    let tpl = Template::try_from("https://trusted.example.com/{{ path }}").unwrap();
    assert!(
        ConfinementChecker::for_uri_template(&tpl)
            .unwrap()
            .is_some()
    );
}

#[test]
fn partial_uri_authority_rejected() {
    // A `{{ field }}` reference inside or directly extending the host.
    for template_str in &[
        "https://tenant.{{ env }}.example.com/",
        "https://api.internal{{ path }}",
    ] {
        let tpl = Template::try_from(*template_str).unwrap();
        assert!(
            matches!(
                ConfinementChecker::for_uri_template(&tpl).unwrap_err(),
                BuildError::PartialUriAuthority { .. }
            ),
            "expected PartialUriAuthority for {template_str}"
        );
    }
}

#[test]
fn root_only_prefix_rejected() {
    // `/{{ tenant }}/` has a literal prefix of `/` — every rendered value
    // trivially starts with it, so it provides no useful confinement.
    let tpl = Template::try_from("/{{ tenant }}/").unwrap();
    assert!(matches!(
        ConfinementChecker::for_prefix_template(&tpl).unwrap_err(),
        BuildError::DerivedBaseIsRoot { .. }
    ));
}

#[test]
fn non_root_slash_prefix_accepted() {
    // `/data/{{ tenant }}/` has a literal prefix of `/data/` — non-root, valid.
    let tpl = Template::try_from("/data/{{ tenant }}/").unwrap();
    let checker = ConfinementChecker::for_prefix_template(&tpl)
        .unwrap()
        .unwrap();
    assert!(checker.confine("/data/tenant-a/").is_ok());
    assert!(checker.confine("/other/tenant-a/").is_err());
}

// Verify that a ConfinedTemplate from confine() renders
// with the checker enforced.
#[test]
fn confined_template_renders_with_check() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event.as_mut_log().insert(event_path!("tenant"), "acme");

    let config = ConfinementConfig::default();
    let tpl = Template::try_from("safe/{{ tenant }}/").unwrap();
    let confined = tpl.confine(&config, "test_sink", "key").unwrap();

    assert_eq!(Ok(Bytes::from("safe/acme/")), confined.render(&event));
}

#[test]
fn confined_template_rejects_escape() {
    let mut event = Event::Log(LogEvent::from("hello world"));
    event
        .as_mut_log()
        .insert(event_path!("tenant"), "../../etc");

    let config = ConfinementConfig::default();
    let tpl = Template::try_from("safe/{{ tenant }}/").unwrap();
    let confined = tpl.confine(&config, "test_sink", "key").unwrap();

    assert!(matches!(
        confined.render(&event).unwrap_err(),
        TemplateRenderingError::Confined { .. }
    ));
}

#[test]
fn http_prefix_treated_as_non_uri_for_confine() {
    // `confine` does not interpret content as URI, so `http://` prefixes
    // are treated as ordinary string prefixes (useful for object-store keys).
    use crate::event::Event;
    use vector_lib::event::LogEvent;

    let mut event = Event::Log(LogEvent::from("x"));
    event
        .as_mut_log()
        .insert(event_path!("region"), "us-east-1");

    let config = ConfinementConfig::default();
    let tpl = Template::try_from("http://logs-{{ region }}/").unwrap();

    // Should succeed - this is prefix confinement, not URI confinement
    let confined = tpl.confine(&config, "test_sink", "key_prefix").unwrap();

    // Should render successfully
    assert_eq!(
        confined.render_string(&event).unwrap(),
        "http://logs-us-east-1/"
    );
}

#[test]
fn uri_template_confine_enforces_uri_semantics() {
    // UriTemplate::confine enforces URI-specific checks
    use crate::event::Event;
    use vector_lib::event::LogEvent;

    let mut event = Event::Log(LogEvent::from("x"));
    event
        .as_mut_log()
        .insert(event_path!("tenant"), "acme/../../../evil");

    let config = ConfinementConfig::default();
    let tpl = UriTemplate::try_from("https://api.example.com/ingest/{{ tenant }}").unwrap();

    // Should succeed to build with URI confinement
    let confined = tpl.confine(&config, "http", "uri").unwrap();

    // But should reject path traversal at render time (URI-specific check)
    assert!(matches!(
        confined.render_string(&event).unwrap_err(),
        TemplateRenderingError::Confined { .. }
    ));
}

#[rstest]
// URIs — dynamic or static — must be absolute http(s) URIs with a fully
// static authority. Reject at build time otherwise.
#[case::relative_uri("/api/{{ tenant }}", "authority")]
#[case::schemeless("//api.example.com/{{ tenant }}", "authority")]
#[case::static_relative_uri("/api/endpoint", "authority")]
#[case::static_schemeless("//api.example.com/path", "authority")]
#[case::unsupported_scheme("ftp://files.example.com/{{ path }}", "ftp")]
#[case::static_unsupported_scheme("ftp://files.example.com/path", "ftp")]
fn uri_template_confine_rejects_invalid_scheme_or_authority(
    #[case] template: &str,
    #[case] err_fragment: &str,
) {
    let config = ConfinementConfig::default();
    let err = UriTemplate::try_from(template)
        .unwrap()
        .confine(&config, "http", "uri")
        .unwrap_err();
    assert!(
        err.to_string().contains(err_fragment),
        "expected error containing {err_fragment:?} for {template}, got {err:?}"
    );
}

#[test]
fn uri_template_confine_opt_out_returns_correct_type() {
    // Even when opting out, UriTemplate::confine must return ConfinedUriTemplate
    let config = ConfinementConfig {
        dangerously_allow_unconfined_template_resolution: true,
    };
    let tpl = UriTemplate::try_from("{{ endpoint }}").unwrap();
    // Type annotation ensures we get ConfinedUriTemplate, not ConfinedTemplate
    let _confined_confined_uri: ConfinedUriTemplate = tpl.confine(&config, "http", "uri").unwrap();
}

#[test]
fn uri_template_confine_accepts_http_and_https_schemes() {
    // Both http:// and https:// must be accepted for UriTemplate, including mixed case
    use crate::event::Event;
    use vector_lib::event::LogEvent;

    let config = ConfinementConfig::default();

    // HTTP scheme (lowercase)
    let mut event = Event::Log(LogEvent::from("x"));
    event.as_mut_log().insert(event_path!("tenant"), "acme");
    let tpl = UriTemplate::try_from("http://api.example.com/{{ tenant }}").unwrap();
    let confined = tpl.confine(&config, "http", "uri").unwrap();
    assert_eq!(
        confined.render_string(&event).unwrap(),
        "http://api.example.com/acme"
    );

    // HTTPS scheme (lowercase)
    let tpl = UriTemplate::try_from("https://api.example.com/{{ tenant }}").unwrap();
    let confined = tpl.confine(&config, "http", "uri").unwrap();
    assert_eq!(
        confined.render_string(&event).unwrap(),
        "https://api.example.com/acme"
    );

    // HTTPS scheme (mixed case - HtTpS)
    let tpl = UriTemplate::try_from("HtTpS://api.example.com/{{ tenant }}").unwrap();
    let confined = tpl.confine(&config, "http", "uri").unwrap();
    assert_eq!(
        confined.render_string(&event).unwrap(),
        "HtTpS://api.example.com/acme"
    );
}

#[test]
fn uri_template_confine_accepts_strftime_only_path() {
    // A URI with only strftime directives and no `{{ }}` references renders
    // operator-authored, time-derived text — nothing is event-controlled, so
    // no runtime checker is built (matching master). It must still build and
    // render; in particular `http::Uri` must not be fed the raw `%Y`/`%m`
    // directives as percent escapes.
    let ts = Utc
        .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
        .single()
        .expect("invalid timestamp");
    let mut event = Event::Log(LogEvent::from("x"));
    event
        .as_mut_log()
        .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

    let config = ConfinementConfig::default();
    let tpl = UriTemplate::try_from("https://logs.example.com/%Y/%m").unwrap();
    let confined = tpl.confine(&config, "http", "uri").unwrap();

    assert_eq!(
        confined.render_string(&event).unwrap(),
        "https://logs.example.com/2001/02"
    );
}

#[test]
fn uri_template_strftime_directive_hash_not_fragment() {
    // `%#z` is one chrono directive (alternate timezone offset form), not a
    // `%` followed by a URI fragment. Field-free strftime templates build
    // with no runtime checker (nothing event-controlled to confine), so the
    // directive is never treated as a fragment delimiter. Note chrono 0.4
    // supports `%#z` for parsing only — rendering it panics — so this test
    // pins build behavior only, exactly like the `%Y/%m` case above.
    let config = ConfinementConfig::default();
    let tpl = UriTemplate::try_from("https://logs.example.com/%#z").unwrap();
    assert!(tpl.confine(&config, "http", "uri").is_ok());
}

#[rstest]
// Static URIs (no `{{ }}`) have fixed, operator-authored values — accepted
// without a runtime checker, including query strings and fragments.
#[case::http("http://api.example.com/path")]
#[case::https("https://secure.example.com/api")]
#[case::with_query("https://api.example.com/ingest?source=vector")]
#[case::with_fragment("https://api.example.com/ingest#section")]
fn static_uri_template_accepts_valid_uri(#[case] uri: &str) {
    let config = ConfinementConfig::default();
    let confined = UriTemplate::try_from(uri)
        .unwrap()
        .confine(&config, "http", "uri")
        .unwrap();
    let event = Event::Log(LogEvent::from("x"));
    assert_eq!(confined.render_string(&event).unwrap(), uri);
}
