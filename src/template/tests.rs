use chrono::{TimeZone, Utc};
use vector_lib::{lookup::metadata_path, metric_tags};
use vrl::event_path;

use super::*;
use crate::event::{Event, LogEvent, MetricKind, MetricValue};

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

    assert_eq!(f1, vec!["foo"]);
    assert_eq!(f2, vec!["foo", "bar"]);
    assert_eq!(f3, None);
    assert_eq!(f4, None);
    assert_eq!(f5, vec!["foo", "bar"]);
    assert_eq!(f6, None);
}

#[test]
fn literal_prefix() {
    let cases = [
        ("/var/log/app.log", "/var/log/app.log"),
        ("/var/log/{{ host }}/app.log", "/var/log/"),
        // `%` is an ordinary literal character now that strftime specifiers
        // are no longer part of the template syntax.
        ("/var/log/%Y/{{ host }}.log", "/var/log/%Y/"),
        ("/srv-{{ id }}.log", "/srv-"),
        ("{{ full_path }}", ""),
        ("/{{ tenant }}/app.log", "/"),
        ("100%%-literal/{{ x }}", "100%%-literal/"),
        ("no-template-at-all", "no-template-at-all"),
        ("only-strftime-%F.log", "only-strftime-%F.log"),
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
        !UnconfinedTemplate::try_from("/kube-demo/%F")
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
fn dotdot_bypass_rejected() {
    // `safe/../../escape` passes a naive starts_with("safe/") check but must
    // be rejected — on filesystem-like protocols (WebHDFS) the server resolves
    // `..` and the value escapes the intended namespace.
    let tpl = Template::try_from("safe/{{ tenant }}/").unwrap();
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
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
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
    assert!(matches!(
        c.confine("https://api.internal/ingest/%252e%252e%252fadmin")
            .unwrap_err(),
        ConfineError::DotDotSegment { .. }
    ));
}

#[test]
fn uri_path_field_cannot_smuggle_query() {
    // Templates with no `?` build successfully. A field value that smuggles
    // `?...` into the path (e.g. tenant=`ok?tenant=evil`) is caught at
    // render time via the query-rejection check.
    let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
    let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
    assert!(matches!(
        c.confine("https://api.internal/ingest/ok?tenant=evil")
            .unwrap_err(),
        ConfineError::OutsideBase { .. }
    ));
    // Fragments are stripped by `http::Uri` before reaching the server,
    // so a rendered `#frag` doesn't count as a query and is accepted.
    assert!(c.confine("https://api.internal/ingest/ok#frag").is_ok());
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
    ] {
        let tpl = Template::try_from(*template_str).unwrap();
        assert!(
            matches!(
                ConfinementChecker::for_template(&tpl).unwrap_err(),
                BuildError::DynamicUriQueryOrFragment { .. }
            ),
            "expected DynamicUriQueryOrFragment for {template_str}"
        );
    }
}

#[test]
fn static_uri_with_query_allowed() {
    // A fully static URI (no `{{ }}`) with a query string is safe: the
    // rendered value is fixed and cannot be influenced by event data.
    // No checker is installed; `Ok(None)` is the expected result.
    let tpl = Template::try_from("https://api.internal/ingest?source=vector").unwrap();
    assert!(ConfinementChecker::for_template(&tpl).unwrap().is_none());
}

#[test]
fn no_static_uri_authority_rejected() {
    // `https://{{ host }}/ingest` has prefix `https://` — any host can be
    // rendered, so the confinement is meaningless. Must be rejected at build.
    for template_str in &["https://{{ host }}/ingest", "http://{{ host }}/path"] {
        let tpl = Template::try_from(*template_str).unwrap();
        assert!(
            matches!(
                ConfinementChecker::for_template(&tpl).unwrap_err(),
                BuildError::NoStaticUriAuthority { .. }
            ),
            "expected NoStaticUriAuthority for {template_str}"
        );
    }
    // A template with a static host followed by a `/` is accepted.
    let tpl = Template::try_from("https://trusted.example.com/{{ path }}").unwrap();
    assert!(ConfinementChecker::for_template(&tpl).unwrap().is_some());
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
                ConfinementChecker::for_template(&tpl).unwrap_err(),
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
        ConfinementChecker::for_template(&tpl).unwrap_err(),
        BuildError::DerivedBaseIsRoot { .. }
    ));
}

#[test]
fn non_root_slash_prefix_accepted() {
    // `/data/{{ tenant }}/` has a literal prefix of `/data/` — non-root, valid.
    let tpl = Template::try_from("/data/{{ tenant }}/").unwrap();
    let checker = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
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
