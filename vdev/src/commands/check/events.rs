//! Native Rust port of the Ruby `scripts/check-events` script.
//!
//! Walks `src/**/*.rs` and `lib/**/*.rs`, extracts internal-event definitions
//! and `tracing` log calls via `syn`'s AST, and validates them against the
//! rules in `docs/specs/instrumentation.md`. Macro argument scraping (the
//! contents of `counter!(...)`, `trace!(...)`, etc.) uses small targeted
//! regexes on the macro's already-tokenised input — never on raw source.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::struct_field_names,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    process,
    sync::LazyLock,
};

use anyhow::Result;
use glob::glob;
use proc_macro2::TokenStream;
use quote::ToTokens;
use regex::Regex;
use syn::{
    ItemImpl, ItemStruct, Type,
    spanned::Spanned,
    visit::{self, Visit},
};

const BYTE_SIZE_COUNT: &[&str] = &["byte_size", "count"];

const METRIC_NAME_EVENTS_DROPPED: &str = "component_discarded_events_total";
const METRIC_NAME_ERROR: &str = "component_errors_total";

struct EventClass {
    /// Required log message text for events with this suffix.
    message: &'static str,
    /// Counter suffixes (full name is `component_<suffix>_total`).
    counters: &'static [&'static str],
    /// Tags that must appear on logs and on counters (minus `BYTE_SIZE_COUNT`).
    additional_tags: &'static [&'static str],
}

const EVENT_CLASSES: &[(&str, EventClass)] = &[
    (
        "BytesReceived",
        EventClass {
            message: "Bytes received.",
            counters: &["received_bytes"],
            additional_tags: &["byte_size", "protocol"],
        },
    ),
    (
        "EventsReceived",
        EventClass {
            message: "Events received.",
            counters: &["received_events", "received_event_bytes"],
            additional_tags: &["count", "byte_size"],
        },
    ),
    (
        "EventsSent",
        EventClass {
            message: "Events sent.",
            counters: &["sent_events", "sent_event_bytes"],
            additional_tags: &["count", "byte_size"],
        },
    ),
    (
        "BytesSent",
        EventClass {
            message: "Bytes sent.",
            counters: &["sent_bytes"],
            additional_tags: &["byte_size", "protocol"],
        },
    ),
];

#[derive(Debug, Default, Clone)]
struct Event {
    path: Option<String>,
    skip_dropped_events: bool,
    skip_duplicate_check: bool,
    skip_validity_check: bool,
    emits_component_events_dropped: bool,
    members: BTreeMap<String, String>,
    counters: BTreeMap<String, BTreeMap<String, String>>,
    metrics: BTreeMap<String, BTreeMap<String, String>>,
    logs: Vec<LogCall>,
    uses: u32,
    impl_internal_event: bool,
    impl_register_event: Option<String>,
    impl_event_handle: bool,
    reports: Vec<String>,
}

#[derive(Debug, Clone)]
struct LogCall {
    level: String,
    message: String,
    parameters: Vec<String>,
}

impl Event {
    fn add_metric(&mut self, ty: &str, name: &str, tags: BTreeMap<String, String>) {
        let key = format!("{ty}:{name}");
        self.metrics.insert(key, tags.clone());
        if ty == "counter" {
            self.counters.insert(name.to_string(), tags);
        }
    }

    fn add_log(&mut self, level: &str, message: &str, parameters: Vec<String>) {
        self.logs.push(LogCall {
            level: level.to_string(),
            message: message.to_string(),
            parameters,
        });
    }

    fn append(&mut self, report: impl Into<String>) {
        self.reports.push(report.into());
    }

    fn signature(&self) -> Option<String> {
        if self.metrics.is_empty() && self.logs.is_empty() {
            return None;
        }
        let members: Vec<String> = self
            .members
            .iter()
            .map(|(name, ty)| format!("{name}:{ty}"))
            .collect();
        let mut metrics: Vec<String> = self
            .metrics
            .iter()
            .map(|(name, tags)| {
                let mut keys: Vec<&str> = tags.keys().map(String::as_str).collect();
                keys.sort_unstable();
                format!("{name}({})", keys.join(","))
            })
            .collect();
        metrics.sort();
        let mut logs: Vec<String> = self
            .logs
            .iter()
            .map(|l| format!("[\"{}\", \"{}\", {:?}]", l.level, l.message, l.parameters))
            .collect();
        logs.sort();
        Some(format!(
            "{}[{}][{}]",
            members.join(":"),
            logs.join(";"),
            metrics.join(";")
        ))
    }
}

// ---- Validation ------------------------------------------------------------

fn name_ends_with(name: &str, suffix: &str) -> bool {
    name.ends_with(suffix)
}

fn log_level_one_of(reports: &mut Vec<String>, logs: &[LogCall], levels: &[&str]) {
    if !logs.iter().any(|l| levels.contains(&l.level.as_str())) {
        reports.push(format!(
            "This event MUST log with one of these levels: [{}].",
            levels
                .iter()
                .map(|l| format!("\"{l}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn counters_must_include_exclude_tags(
    reports: &mut Vec<String>,
    counters: &BTreeMap<String, BTreeMap<String, String>>,
    name: &str,
    required_tags: &[&str],
    exclude_tags: &[&str],
) {
    let Some(tags) = counters.get(name) else {
        reports.push(format!("This event MUST increment counter \"{name}\"."));
        return;
    };
    for tag in required_tags {
        if !tags.contains_key(*tag) {
            reports.push(format!("Counter \"{name}\" MUST include tag \"{tag}\"."));
        }
    }
    for tag in exclude_tags {
        if tags.contains_key(*tag) {
            reports.push(format!(
                "Counter \"{name}\" MUST NOT include tag \"{tag}\"."
            ));
        }
    }
}

fn validate_event(events: &HashMap<String, Event>, name: &str, handle_name: &str) -> Vec<String> {
    let event = events.get(name).expect("event present");
    let handle = events.get(handle_name).expect("handle present");
    let mut reports: Vec<String> = Vec::new();

    if event.uses == 0 {
        reports.push("Event has no uses.".to_string());
    }

    for (suffix, class) in EVENT_CLASSES {
        if !name_ends_with(name, suffix) {
            continue;
        }
        for log in &handle.logs {
            if log.level != "trace" {
                reports.push("Log type MUST be \"trace!\".".to_string());
            }
            if log.message != class.message {
                reports.push(format!(
                    "Log message MUST be \"{}\" (is \"{}\").",
                    class.message, log.message
                ));
            }
            for tag in class.additional_tags {
                if !log.parameters.iter().any(|p| p == tag) {
                    reports.push(format!("Log MUST contain tag \"{tag}\""));
                }
            }
        }
        for counter in class.counters {
            let counter_name = format!("component_{counter}_total");
            let required: Vec<&str> = class
                .additional_tags
                .iter()
                .copied()
                .filter(|t| !BYTE_SIZE_COUNT.contains(t))
                .collect();
            counters_must_include_exclude_tags(
                &mut reports,
                &event.counters,
                &counter_name,
                &required,
                &[],
            );
        }
    }

    let has_error_logs = handle.logs.iter().filter(|l| l.level == "error").count() == 1;
    let is_events_dropped_event = name_ends_with(name, "EventsDropped")
        || event.counters.contains_key(METRIC_NAME_EVENTS_DROPPED);

    if (has_error_logs && !is_events_dropped_event) || name_ends_with(name, "Error") {
        if !name_ends_with(name, "Error") {
            reports.push("Error events MUST be named \"___Error\".".to_string());
        }
        log_level_one_of(&mut reports, &handle.logs, &["error"]);
        counters_must_include_exclude_tags(
            &mut reports,
            &event.counters,
            METRIC_NAME_ERROR,
            &["error_type", "stage"],
            &[],
        );
        for log in &handle.logs {
            if log.level != "error" {
                continue;
            }
            for parameter in ["error_type", "stage"] {
                if !log.parameters.iter().any(|p| p == parameter) {
                    reports.push(format!(
                        "Error log for Error event MUST include parameter \"{parameter}\"."
                    ));
                }
            }
            for parameter in ["error_code", "error_type", "stage"] {
                if log.parameters.iter().any(|p| p == parameter)
                    && !event
                        .counters
                        .get(METRIC_NAME_ERROR)
                        .is_some_and(|m| m.contains_key(parameter))
                {
                    reports.push(format!(
                        "Counter \"{METRIC_NAME_ERROR}\" must include \"{parameter}\" to match error log."
                    ));
                }
            }
        }
    }

    if is_events_dropped_event && !event.skip_dropped_events {
        if event.emits_component_events_dropped {
            if event.counters.contains_key(METRIC_NAME_EVENTS_DROPPED) {
                reports.push(format!(
                    "Event emitting ComponentEventsDropped should not also increment counter `{METRIC_NAME_EVENTS_DROPPED}`"
                ));
            }
        } else {
            if !name_ends_with(name, "EventsDropped") {
                reports
                    .push("EventsDropped events MUST be named \"___EventsDropped\".".to_string());
            }
            log_level_one_of(&mut reports, &handle.logs, &["error", "debug"]);
            counters_must_include_exclude_tags(
                &mut reports,
                &event.counters,
                METRIC_NAME_EVENTS_DROPPED,
                &["intentional"],
                &["reason", "count"],
            );
            for log in &handle.logs {
                if log.level != "error" {
                    continue;
                }
                for parameter in ["count", "intentional", "reason"] {
                    if !log.parameters.iter().any(|p| p == parameter) {
                        reports.push(format!(
                            "Error log for EventsDropped event MUST include parameter \"{parameter}\"."
                        ));
                    }
                }
                if log.parameters.iter().any(|p| p == "intentional")
                    && !event
                        .counters
                        .get(METRIC_NAME_EVENTS_DROPPED)
                        .is_some_and(|m| m.contains_key("intentional"))
                {
                    reports.push(format!(
                        "Counter \"{METRIC_NAME_EVENTS_DROPPED}\" must include \"intentional\" to match error log."
                    ));
                }
            }
        }
    }

    for (cname, tags) in &event.counters {
        if cname != METRIC_NAME_ERROR && cname != METRIC_NAME_EVENTS_DROPPED {
            continue;
        }
        for (tag, value) in tags {
            if tag == "stage" && !value.starts_with("error_stage::") {
                reports.push(format!(
                    "Counter \"{cname}\" tag \"{tag}\" value must be an \"error_stage\" constant."
                ));
            } else if tag == "error_type" && !value.starts_with("error_type::") {
                reports.push(format!(
                    "Counter \"{cname}\" tag \"{tag}\" value must be an \"error_type\" constant."
                ));
            }
        }
    }

    for r in &event.reports {
        reports.push(r.clone());
    }

    reports
}

// ---- Macro arg parsers (operate on small token strings) --------------------

/// `emit!(ComponentEventsDropped...)` detection regex, applied to the raw
/// source slice of an impl block (which preserves comments and original
/// formatting that `to_token_stream` strips).
static RE_EMIT_DROPPED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:emit|register)!\([ \t\r\n]*ComponentEventsDropped(?:[^A-Za-z0-9_]|$)").unwrap()
});

/// `emit!(EventName)` / `register!(Path::EventName)` use-counting regex,
/// applied to the raw file text so it sees calls nested inside other macros
/// (e.g. `tokio::select!`) that `syn` does not descend into.
static RE_USES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^A-Za-z0-9_])(?:emit!?|register!?)\((?:[a-z][a-z0-9_:]+)?([A-Z][A-Za-z0-9]+)",
    )
    .unwrap()
});

/// `"key" => value` tag-pair regex. Used inside `counter!(...)` arg lists.
/// Note: syn's `TokenStream` rendering may produce `=>` as `= >`; the regex
/// accepts either form via `=[ \t]*>`.
static RE_TAG_PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""([^"]+)"[ \t\r\n]*=[ \t\r\n]*>[ \t\r\n]*(.+?)(?:,|$)"#).unwrap()
});

/// Strip whitespace introduced by `TokenStream::to_string` around `::` so
/// `error_stage :: PROCESSING` becomes `error_stage::PROCESSING` for the
/// constant-prefix validation (`starts_with("error_stage::")`).
fn normalize_value(s: &str) -> String {
    let trimmed = s.trim();
    let collapsed = Regex::new(r"[ \t\r\n]*::[ \t\r\n]*")
        .unwrap()
        .replace_all(trimmed, "::");
    collapsed.into_owned()
}

/// Convert `CamelCase` to `snake_case` to mirror the Ruby variant→metric name
/// mapping (`CounterName::ComponentErrorsTotal` → `component_errors_total`).
fn camel_to_snake(name: &str) -> String {
    let pass1 = Regex::new(r"([A-Z]+)([A-Z][a-z])")
        .unwrap()
        .replace_all(name, "${1}_${2}");
    let pass2 = Regex::new(r"([a-z0-9])([A-Z])")
        .unwrap()
        .replace_all(&pass1, "${1}_${2}");
    pass2.to_lowercase()
}

/// Split a token stream that represents a comma-separated argument list into
/// per-argument substrings, respecting bracket/paren/brace nesting and string
/// literals. Operates on the (already-bounded) macro-arg token text.
fn split_comma_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim().to_string();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

#[derive(Debug)]
struct ParsedMetric {
    ty: String,
    name: String,
    tags: BTreeMap<String, String>,
}

/// Parse a `counter!(...)` / `gauge!(...)` / `histogram!(...)` invocation's
/// already-tokenised args into a name (string literal or `CamelCase` variant
/// of `<X>Name::Variant`) and its `"key" => value` tag pairs.
fn parse_metric_args(ty: &str, tokens: &TokenStream) -> Option<ParsedMetric> {
    let raw = tokens.to_string();
    let args = split_comma_args(&raw);
    if args.is_empty() {
        return None;
    }
    let name = parse_metric_name(args[0].as_str())?;
    let mut tags = BTreeMap::new();
    let rest = args[1..].join(",");
    for caps in RE_TAG_PAIR.captures_iter(&rest) {
        tags.insert(caps[1].to_string(), normalize_value(&caps[2]));
    }
    if tags.is_empty() {
        for caps in RE_TAG_PAIR.captures_iter(&raw) {
            tags.insert(caps[1].to_string(), normalize_value(&caps[2]));
        }
    }
    Some(ParsedMetric {
        ty: ty.to_string(),
        name,
        tags,
    })
}

/// Extract the metric name from the first arg of a `counter!`/`gauge!`/`histogram!`.
/// Accepts `"literal"` or `<TypeName>::<Variant>`.
fn parse_metric_name(arg: &str) -> Option<String> {
    let arg = arg.trim();
    if let Some(stripped) = arg.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(stripped.to_string());
    }
    // path::Variant — Ruby matched `\w+Name::(\w+)` and snake-cased the variant.
    let re = Regex::new(r"^[A-Za-z0-9_]+Name[ \t]*::[ \t]*([A-Za-z0-9_]+)").unwrap();
    re.captures(arg).map(|c| camel_to_snake(&c[1]))
}

#[derive(Debug)]
struct ParsedLog {
    /// The captured message text (string literal contents *or* variable name
    /// if the message was passed as an expression). Always set when the log
    /// has any message-shaped argument.
    message: String,
    /// Whether the message came from a string literal (`"..."`). Format
    /// checks (capitalised, trailing period) only run on literal messages.
    has_literal_message: bool,
    parameters: Vec<String>,
}

/// Parse a `trace!(...)` / `debug!(...)` / `info!(...)` / `warn!(...)` /
/// `error!(...)` invocation's tokens into the message text and the list of
/// parameter names it carries.
///
/// `tracing` allows the message in any position: leading positional literal,
/// trailing positional literal, or `message = "..."` named field. We mirror
/// the Ruby script: take the first arg that looks like a string literal (or
/// `message = "literal"`) and treat it as the message.
fn parse_log_args(tokens: &TokenStream) -> ParsedLog {
    let raw = tokens.to_string();
    let args = split_comma_args(&raw);

    let mut message: Option<String> = None;
    let mut has_literal_message = false;
    let mut parameters = Vec::new();

    for arg in &args {
        let trimmed = arg.trim();
        if trimmed.starts_with("target :") || trimmed.starts_with("parent :") {
            continue;
        }
        if message.is_none() {
            // `message = ...` (named field; value may or may not be a literal).
            if let Some(rest) = trimmed.strip_prefix("message") {
                let rest = rest.trim_start();
                if let Some(value) = rest.strip_prefix('=').map(str::trim_start) {
                    let value = value.trim();
                    if let Some(stripped) =
                        value.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
                    {
                        message = Some(stripped.to_string());
                        has_literal_message = true;
                    } else {
                        message = Some(value.to_string());
                    }
                    continue;
                }
            }
            // Leading positional string literal.
            if let Some(stripped) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                message = Some(stripped.to_string());
                has_literal_message = true;
                continue;
            }
            // Bare positional expression (variable). Take the first one as the
            // message, *and* record it as a parameter (it serves both roles in
            // tracing's API).
            if !trimmed.contains('=') && message.is_none() {
                message = Some(trimmed.to_string());
                if let Some(name) = parameter_name(trimmed) {
                    parameters.push(name);
                }
                continue;
            }
        }
        if let Some(name) = parameter_name(trimmed) {
            parameters.push(name);
        }
    }

    ParsedLog {
        message: message.unwrap_or_default(),
        has_literal_message,
        parameters,
    }
}

/// Extract the parameter name from a log-macro arg. `tracing` accepts:
/// `name = expr`, `?name`, `%name`, and bare `name`. Token-stream
/// serialisation puts whitespace around punctuation (`% protocol`) so we
/// trim after stripping each prefix.
fn parameter_name(arg: &str) -> Option<String> {
    let s = arg.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((lhs, _)) = s.split_once('=') {
        let lhs = lhs
            .trim()
            .trim_start_matches('?')
            .trim_start_matches('%')
            .trim();
        if is_identifier(lhs) {
            return Some(lhs.to_string());
        }
    }
    let stripped = s
        .trim_start_matches('?')
        .trim_start_matches('%')
        .trim_start();
    let head: String = stripped
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    if !head.is_empty() && head.chars().any(|c| c.is_ascii_alphabetic() || c == '_') {
        return Some(head);
    }
    None
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

// ---- AST scanner -----------------------------------------------------------

#[derive(Clone)]
struct ImplCtx {
    event_name: String,
}

struct Scanner<'a> {
    events: &'a mut HashMap<String, Event>,
    path_str: String,
    in_internal_events_dir: bool,
    in_src_dir: bool,
    skip_dropped_for_file: bool,
    text: &'a str,
    impl_stack: Vec<ImplCtx>,
    format_reports: Vec<String>,
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if self.in_internal_events_dir {
            let name = node.ident.to_string();
            let event = self.events.entry(name).or_default();
            event.path = Some(self.path_str.clone());
            event.skip_dropped_events = self.skip_dropped_for_file;
            for field in &node.fields {
                if let Some(ident) = &field.ident {
                    let ty = field.ty.to_token_stream().to_string();
                    event.members.insert(ident.to_string(), ty);
                }
            }
        }
        visit::visit_item_struct(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let trait_name = node
            .trait_
            .as_ref()
            .and_then(|(_, path, _)| path.segments.last())
            .map(|s| s.ident.to_string());
        let event_name = match &*node.self_ty {
            Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
            _ => None,
        };

        if self.in_internal_events_dir
            && let (Some(trait_name), Some(event_name)) = (trait_name.as_deref(), event_name)
        {
            let mut handled = false;
            if matches!(
                trait_name,
                "InternalEvent" | "RegisterInternalEvent" | "InternalEventHandle"
            ) {
                // The token-stream form of the impl block has all comments
                // stripped by syn, so the `## skip ##` markers (which live in
                // line comments) are missing. Read them from the original
                // source text within the span instead.
                let raw_block = source_slice(self.text, node.span());
                let registers_inside = node.to_token_stream().to_string().contains("register (");

                let event = self.events.entry(event_name.clone()).or_default();
                event.path = Some(self.path_str.clone());
                event.skip_duplicate_check |=
                    raw_block.contains("## skip check-duplicate-events ##");
                event.skip_validity_check |= raw_block.contains("## skip check-validity-events ##");

                match trait_name {
                    "InternalEvent" => {
                        if !registers_inside {
                            event.impl_internal_event = true;
                        }
                    }
                    "RegisterInternalEvent" => {
                        event.impl_register_event = Some(event_name.clone());
                        event.append(
                            "Do not implement RegisterInternalEvent manually. Use the registered_event! macro instead.",
                        );
                    }
                    "InternalEventHandle" => event.impl_event_handle = true,
                    _ => {}
                }
                if RE_EMIT_DROPPED.is_match(&raw_block) {
                    event.emits_component_events_dropped = true;
                }
                self.impl_stack.push(ImplCtx { event_name });
                handled = true;
            }
            visit::visit_item_impl(self, node);
            if handled {
                self.impl_stack.pop();
            }
            return;
        }
        visit::visit_item_impl(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = node
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();

        // Format-check log messages everywhere in `src/`.
        // (Use-counting and ComponentEventsDropped detection happen via a
        // separate raw-text pass because `syn` does not recurse into the
        // bodies of arbitrary `tokio::select!` / `cfg_if!` / etc. macro
        // invocations, so an AST-only walk misses nested `emit!` calls.)
        if self.in_src_dir && matches!(name.as_str(), "trace" | "debug" | "info" | "warn" | "error")
        {
            self.format_check_log(node, &name);
        }

        // Inside an InternalEvent-family impl: capture logs / metrics /
        //    ComponentEventsDropped emissions for the active event.
        if let Some(ctx) = self.impl_stack.last().cloned() {
            match name.as_str() {
                "trace" | "debug" | "info" | "warn" | "error" => {
                    let parsed = parse_log_args(&node.tokens);
                    let event = self.events.entry(ctx.event_name.clone()).or_default();
                    event.add_log(&name, &parsed.message, parsed.parameters);
                }
                "counter" | "gauge" | "histogram" => {
                    if let Some(metric) = parse_metric_args(&name, &node.tokens) {
                        let event = self.events.entry(ctx.event_name.clone()).or_default();
                        event.add_metric(&metric.ty, &metric.name, metric.tags);
                    }
                }
                _ => {}
            }
        }
        if name == "registered_event" {
            self.handle_registered_event(node);
        }

        visit::visit_macro(self, node);
    }
}

impl Scanner<'_> {
    fn format_check_log(&mut self, mac: &syn::Macro, level: &str) {
        let parsed = parse_log_args(&mac.tokens);
        // Format checks (capitalisation, trailing period) only apply to
        // string-literal messages — variable-message expressions are opaque.
        if !parsed.has_literal_message {
            return;
        }
        let message = parsed.message;
        if message.is_empty() {
            return;
        }
        let is_capitalized = message.starts_with('{')
            || !message
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            || message
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase());
        let has_trailing_period = message.ends_with('}') || message.ends_with('.');
        if is_capitalized && has_trailing_period {
            return;
        }
        let line_no = mac.span().start().line;
        if !is_capitalized {
            self.format_reports.push(format!(
                "    Message must start with a capital. (`{level}` call on {}:{line_no})",
                self.path_str
            ));
        }
        if !has_trailing_period {
            self.format_reports.push(format!(
                "    Message must end with a period. (`{level}` call on {}:{line_no})",
                self.path_str
            ));
        }
        let _ = self.text;
    }

    /// Parse a `registered_event!` invocation's tokens to extract the event
    /// name, members, handle metrics, and emit-block log calls.
    fn handle_registered_event(&mut self, mac: &syn::Macro) {
        let raw = mac.tokens.to_string();
        // Event name: first ident.
        let Some(event_name) = first_ident(&raw) else {
            return;
        };
        let event = self.events.entry(event_name.clone()).or_default();
        event.path = Some(self.path_str.clone());

        // Pull out the optional `{ event_fields }` immediately after the name,
        // then `=> { handle_fields }`, and the `fn emit(...)  { body }`.
        let after_name = match raw.find(&event_name) {
            Some(idx) => &raw[idx + event_name.len()..],
            None => return,
        };
        let after_name = after_name.trim_start();

        // Extract `{ ... }` after name, if any (optional event fields).
        let (event_fields_text, after_fields): (Option<String>, &str) =
            if after_name.starts_with('{') {
                let (block, rest) = split_brace_block(after_name);
                (Some(block.to_string()), rest)
            } else {
                (None, after_name)
            };

        // Parse member fields from the event-fields block.
        if let Some(block) = event_fields_text {
            for arg in split_comma_args(&block) {
                if let Some((name, ty)) = arg.split_once(':') {
                    event
                        .members
                        .insert(name.trim().to_string(), ty.trim().to_string());
                }
            }
        }

        // Skip past `=> { handle_fields }`.
        let after_arrow = after_fields.trim_start();
        let after_arrow = after_arrow
            .strip_prefix("=>")
            .unwrap_or(after_arrow)
            .trim_start();
        let (handle_block, _after_handle) = if after_arrow.starts_with('{') {
            let (block, rest) = split_brace_block(after_arrow);
            (block.to_string(), rest)
        } else {
            return;
        };

        // Each handle field: `name : type = expr ,`. Pick out metric calls
        // inside the `expr` portion to register on the event.
        for arg in split_comma_args(&handle_block) {
            let arg = arg.trim();
            if arg.is_empty() {
                continue;
            }
            // Attempt to parse the assignment.
            let after_colon = match arg.find(':') {
                Some(i) => &arg[i + 1..],
                None => continue,
            };
            let Some((_ty, expr)) = after_colon.split_once('=') else {
                continue;
            };
            let expr = expr.trim();

            // Look for embedded `counter!` / `gauge!` / `histogram!` calls.
            for ty in ["counter", "gauge", "histogram"] {
                let needle = format!("{ty} ! (");
                if let Some(idx) = expr.find(&needle) {
                    // Find the matching `)` from the `(` after `!`.
                    let after = &expr[idx + needle.len()..];
                    if let Some(end) = match_paren_end(after) {
                        let inside = &after[..end];
                        let toks: TokenStream = inside.parse().unwrap_or_default();
                        if let Some(metric) = parse_metric_args(ty, &toks) {
                            event.add_metric(&metric.ty, &metric.name, metric.tags);
                        }
                    }
                }
            }

            // Component-events-dropped emission.
            if expr.contains("emit ! (ComponentEventsDropped")
                || expr.contains("register ! (ComponentEventsDropped")
            {
                event.emits_component_events_dropped = true;
            }
        }

        // The emit-fn body. Find `fn emit (...) { ... }` after the handle block.
        // We re-scan the original tokens for any log macros within the impl
        // body via the AST visitor — simpler than reparsing here. The handle
        // block above already covers metric extraction. Logs registered to the
        // outer event come from the visit_macro handling above when the visitor
        // descends into nested macros (note: macros aren't normal items, so
        // visit_macro won't recurse into a parent macro's tokens). To capture
        // log calls inside `registered_event!`, we parse them out by scanning
        // the macro's full token text for log macro signatures.
        for ty in ["trace", "debug", "info", "warn", "error"] {
            let needle = format!("{ty} ! (");
            let mut start = 0;
            while let Some(idx) = raw[start..].find(&needle) {
                let after = &raw[start + idx + needle.len()..];
                if let Some(end) = match_paren_end(after) {
                    let inside = &after[..end];
                    let toks: TokenStream = inside.parse().unwrap_or_default();
                    let parsed = parse_log_args(&toks);
                    let event = self.events.entry(event_name.clone()).or_default();
                    event.add_log(ty, &parsed.message, parsed.parameters);
                    start = start + idx + needle.len() + end;
                } else {
                    break;
                }
            }
        }
    }
}

/// Extract the source slice covered by a `proc_macro2::Span`. Used to read
/// line-comment skip markers (e.g. `## skip check-validity-events ##`) which
/// `syn` discards from the AST.
fn source_slice(text: &str, span: proc_macro2::Span) -> String {
    let start = span.start();
    let end = span.end();
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        if line_no >= start.line && line_no <= end.line {
            out.push_str(line);
            out.push('\n');
        }
        if line_no > end.line {
            break;
        }
    }
    out
}

/// Given a string starting with `(`, find the index of the matching `)`.
fn match_paren_end(s: &str) -> Option<usize> {
    // `s` here is the text right after the opening `(`. Walk it tracking depth.
    let mut depth: i32 = 1;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in s.bytes().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a `{ ... }` block off the front of `s`, returning `(inside, rest)`.
fn split_brace_block(s: &str) -> (&str, &str) {
    if !s.starts_with('{') {
        return ("", s);
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in s.bytes().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (&s[1..i], &s[i + 1..]);
                }
            }
            _ => {}
        }
    }
    ("", s)
}

/// Pull the first identifier-shaped substring out of a token text.
fn first_ident(s: &str) -> Option<String> {
    for tok in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if !tok.is_empty()
            && tok
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return Some(tok.to_string());
        }
    }
    None
}

// ---- CLI -------------------------------------------------------------------

/// Check that internal events satisfy the patterns set in
/// <https://github.com/vectordotdev/vector/blob/master/docs/specs/instrumentation.md>.
#[derive(clap::Args, Debug)]
#[command()]
pub(super) struct Cli {}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        let mut events: HashMap<String, Event> = HashMap::new();
        let mut error_count = 0usize;
        let mut all_format_reports: Vec<String> = Vec::new();

        let mut paths: Vec<PathBuf> = Vec::new();
        for pattern in ["src/**/*.rs", "lib/**/*.rs"] {
            for entry in glob(pattern)? {
                paths.push(entry?);
            }
        }
        paths.sort();

        for path in &paths {
            let path_str = path.to_string_lossy().replace('\\', "/");
            let text = fs::read_to_string(path)?;
            let lower = text.to_ascii_lowercase();

            let in_internal_events = path_str.starts_with("src/internal_events/")
                || path_str.starts_with("lib/vector-common/src/internal_event/");
            let in_src = path_str.starts_with("src/");
            let skip_dropped = lower.contains("## skip check-dropped-events ##");

            // Use-counting pass: scrape `emit!(EventName)` / `register!(Path::EventName)`
            // anywhere in the file, including inside other macros (`tokio::select!`
            // etc.), which `syn` does not descend into.
            for caps in RE_USES.captures_iter(&text) {
                let name = caps[1].to_string();
                events.entry(name).or_default().uses += 1;
            }

            let file = match syn::parse_file(&text) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("warning: failed to parse {path_str}: {e}");
                    continue;
                }
            };

            let mut scanner = Scanner {
                events: &mut events,
                path_str: path_str.clone(),
                in_internal_events_dir: in_internal_events,
                in_src_dir: in_src,
                skip_dropped_for_file: skip_dropped,
                text: &text,
                impl_stack: Vec::new(),
                format_reports: Vec::new(),
            };
            visit::visit_file(&mut scanner, &file);
            let count = scanner.format_reports.len();
            if count > 0 {
                for r in &scanner.format_reports {
                    println!("{r}");
                }
                error_count += count;
            }
            all_format_reports.extend(scanner.format_reports);
        }

        // Validation phase.
        let mut names: Vec<String> = events.keys().cloned().collect();
        names.sort();
        let mut duplicates: HashMap<String, Vec<String>> = HashMap::new();

        for name in &names {
            let event = events.get(name).expect("present").clone();
            if !event.skip_duplicate_check
                && (event.impl_internal_event || event.impl_event_handle)
                && let Some(sig) = event.signature()
            {
                duplicates.entry(sig).or_default().push(name.clone());
            }
            if event.skip_validity_check {
                continue;
            }
            if event.impl_internal_event {
                let reports = validate_event(&events, name, name);
                if !reports.is_empty() {
                    let path = events
                        .get(name)
                        .and_then(|e| e.path.as_deref())
                        .unwrap_or("?");
                    println!("{path}: Errors in event {name}:");
                    for r in &reports {
                        println!("    {r}");
                    }
                    error_count += 1;
                }
            } else if let Some(handle_name) = event.impl_register_event.as_deref() {
                if events.contains_key(handle_name) {
                    let reports = validate_event(&events, name, handle_name);
                    if !reports.is_empty() {
                        let path = events
                            .get(name)
                            .and_then(|e| e.path.as_deref())
                            .unwrap_or("?");
                        println!("{path}: Errors in event {name}:");
                        for r in &reports {
                            println!("    {r}");
                        }
                        error_count += 1;
                    }
                } else {
                    println!("Registered event {name} references nonexistent handle {handle_name}");
                    error_count += 1;
                }
            }
        }

        let mut dup_keys: Vec<&String> = duplicates.keys().collect();
        dup_keys.sort();
        for sig in dup_keys {
            let dupes = &duplicates[sig];
            if dupes.len() > 1 {
                println!("Duplicate events detected: {}", dupes.join(", "));
                error_count += 1;
            }
        }

        println!("{error_count} error(s)");
        if error_count > 0 {
            process::exit(1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_to_snake_basic() {
        assert_eq!(camel_to_snake("BytesReceived"), "bytes_received");
        assert_eq!(camel_to_snake("HTTPRequest"), "http_request");
        assert_eq!(camel_to_snake("ABCDef"), "abc_def");
    }

    #[test]
    fn split_comma_args_respects_nesting() {
        assert_eq!(
            split_comma_args(r#""a", "b" => "c, d", e"#),
            vec![
                r#""a""#.to_string(),
                r#""b" => "c, d""#.to_string(),
                "e".to_string(),
            ]
        );
    }

    #[test]
    fn parse_metric_name_string_or_variant() {
        assert_eq!(
            parse_metric_name(r#""my_metric""#),
            Some("my_metric".to_string())
        );
        assert_eq!(
            parse_metric_name("CounterName::ComponentErrorsTotal"),
            Some("component_errors_total".to_string())
        );
        assert_eq!(parse_metric_name("not_a_metric"), None);
    }

    #[test]
    fn signature_none_when_empty() {
        assert!(Event::default().signature().is_none());
    }

    fn parse(src: &str) -> ParsedLog {
        let mac: syn::Macro = syn::parse_str(src).expect("parse macro");
        parse_log_args(&mac.tokens)
    }

    #[test]
    fn parse_log_args_literal_message_first() {
        let p = parse(r#"trace!("Hello there.", count = 1)"#);
        assert_eq!(p.message, "Hello there.");
        assert!(p.has_literal_message);
        assert_eq!(p.parameters, vec!["count".to_string()]);
    }

    #[test]
    fn parse_log_args_literal_message_named() {
        let p = parse(r#"error!(message = "Stuff broke.", error_type = err)"#);
        assert_eq!(p.message, "Stuff broke.");
        assert!(p.has_literal_message);
        assert_eq!(p.parameters, vec!["error_type".to_string()]);
    }

    #[test]
    fn parse_log_args_variable_message_named() {
        let p = parse("error!(message = exec_reason, error_type = err, stage = stg)");
        assert_eq!(p.message, "exec_reason");
        assert!(!p.has_literal_message);
        assert_eq!(
            p.parameters,
            vec!["error_type".to_string(), "stage".to_string()]
        );
    }

    #[test]
    fn parse_log_args_trailing_string_literal() {
        // Some sites pass key=values first and the literal message last —
        // tracing accepts this.
        let p = parse(r#"error!(path = req.uri().path(), "Bad request.")"#);
        assert_eq!(p.message, "Bad request.");
        assert!(p.has_literal_message);
        assert!(p.parameters.contains(&"path".to_string()));
    }

    #[test]
    fn parse_log_args_percent_capture() {
        let p = parse(r#"trace!(message = "Bytes received.", byte_size = bs, %protocol)"#);
        assert!(p.has_literal_message);
        assert_eq!(p.message, "Bytes received.");
        assert!(p.parameters.contains(&"byte_size".to_string()));
        assert!(p.parameters.contains(&"protocol".to_string()));
    }

    fn check(message: &str) -> (bool, bool) {
        // Replicates the gating in `format_check_log` for a literal message.
        let is_capitalized = message.starts_with('{')
            || !message
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
            || message
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false);
        let has_trailing_period = message.ends_with('}') || message.ends_with('.');
        (is_capitalized, has_trailing_period)
    }

    #[test]
    fn message_format_capital_period_pass() {
        assert_eq!(check("Hello there."), (true, true));
    }

    #[test]
    fn message_format_lowercase_first_fails() {
        let (cap, _) = check("hello there.");
        assert!(!cap);
    }

    #[test]
    fn message_format_no_period_fails() {
        let (_, period) = check("Hello there");
        assert!(!period);
    }

    #[test]
    fn message_format_interpolation_passes() {
        // `{...}` at start or end is fine — we can't see what it expands to.
        assert_eq!(check("{count} dropped."), (true, true));
        assert_eq!(check("Dropped {count}"), (true, true));
    }

    #[test]
    fn message_format_non_alpha_first_passes() {
        // E.g. starts with a number — no capitalisation requirement.
        assert_eq!(check("42 things happened."), (true, true));
    }

    // ---- validate_event branch coverage --------------------------------
    //
    // Each test builds a synthetic `Event` (or a `(event, handle)` pair for
    // registered events), inserts it into a HashMap, calls `validate_event`,
    // and asserts on the returned report list. This covers the rule branches
    // independently of the parsing/scanning layer.

    fn mk_event() -> Event {
        Event {
            uses: 1, // default to "has uses" so that branch isn't always firing
            impl_internal_event: true,
            ..Default::default()
        }
    }

    fn one_log(level: &str, message: &str, params: &[&str]) -> Vec<LogCall> {
        vec![LogCall {
            level: level.to_string(),
            message: message.to_string(),
            parameters: params.iter().map(|s| (*s).to_string()).collect(),
        }]
    }

    fn counter(tags: &[(&str, &str)]) -> BTreeMap<String, String> {
        tags.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn run(name: &str, event: Event) -> Vec<String> {
        let mut events = HashMap::new();
        events.insert(name.to_string(), event);
        validate_event(&events, name, name)
    }

    #[test]
    fn validate_event_no_uses_reported() {
        let mut e = mk_event();
        e.uses = 0;
        let r = run("Foo", e);
        assert!(r.iter().any(|m| m == "Event has no uses."));
    }

    #[test]
    fn validate_bytes_received_log_type_must_be_trace() {
        let mut e = mk_event();
        e.logs = one_log("info", "Bytes received.", &["byte_size", "protocol"]);
        e.counters.insert(
            "component_received_bytes_total".to_string(),
            counter(&[("protocol", "tcp")]),
        );
        let r = run("FooBytesReceived", e);
        assert!(r.iter().any(|m| m == "Log type MUST be \"trace!\"."));
    }

    #[test]
    fn validate_bytes_received_log_message_exact() {
        let mut e = mk_event();
        e.logs = one_log(
            "trace",
            "Bytes were received here.",
            &["byte_size", "protocol"],
        );
        e.counters.insert(
            "component_received_bytes_total".to_string(),
            counter(&[("protocol", "tcp")]),
        );
        let r = run("FooBytesReceived", e);
        assert!(
            r.iter()
                .any(|m| m.contains("Log message MUST be \"Bytes received.\""))
        );
    }

    #[test]
    fn validate_bytes_received_log_required_tag() {
        let mut e = mk_event();
        e.logs = one_log("trace", "Bytes received.", &["byte_size"]); // missing protocol
        e.counters.insert(
            "component_received_bytes_total".to_string(),
            counter(&[("protocol", "tcp")]),
        );
        let r = run("FooBytesReceived", e);
        assert!(r.iter().any(|m| m == "Log MUST contain tag \"protocol\""));
    }

    #[test]
    fn validate_bytes_received_counter_required_tag() {
        let mut e = mk_event();
        e.logs = one_log("trace", "Bytes received.", &["byte_size", "protocol"]);
        e.counters
            .insert("component_received_bytes_total".to_string(), counter(&[])); // missing protocol
        let r = run("FooBytesReceived", e);
        assert!(r.iter().any(|m| {
            m == "Counter \"component_received_bytes_total\" MUST include tag \"protocol\"."
        }));
    }

    #[test]
    fn validate_events_received_class() {
        let mut e = mk_event();
        e.logs = one_log("trace", "Wrong message.", &["count", "byte_size"]);
        let r = run("FooEventsReceived", e);
        assert!(
            r.iter()
                .any(|m| m.contains("Log message MUST be \"Events received.\""))
        );
        assert!(
            r.iter()
                .any(|m| m
                    == "This event MUST increment counter \"component_received_events_total\".")
        );
    }

    #[test]
    fn validate_error_event_must_be_named_error() {
        let mut e = mk_event();
        e.logs = one_log("error", "Something failed.", &["error_type", "stage"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("BadlyNamed", e);
        assert!(
            r.iter()
                .any(|m| m == "Error events MUST be named \"___Error\".")
        );
    }

    #[test]
    fn validate_error_event_log_level_must_be_error() {
        let mut e = mk_event();
        // info-level log when name ends with Error
        e.logs = one_log("info", "Something failed.", &["error_type", "stage"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("FooError", e);
        assert!(
            r.iter()
                .any(|m| m.contains("MUST log with one of these levels: [\"error\"]"))
        );
    }

    #[test]
    fn validate_error_event_log_must_include_error_type_and_stage() {
        let mut e = mk_event();
        e.logs = one_log("error", "Something failed.", &[]); // no error_type, no stage
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("FooError", e);
        assert!(
            r.iter()
                .any(|m| m == "Error log for Error event MUST include parameter \"error_type\".")
        );
        assert!(
            r.iter()
                .any(|m| m == "Error log for Error event MUST include parameter \"stage\".")
        );
    }

    #[test]
    fn validate_error_counter_must_match_error_log_params() {
        let mut e = mk_event();
        // Log mentions error_code but counter doesn't
        e.logs = one_log("error", "Failed.", &["error_type", "stage", "error_code"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("FooError", e);
        assert!(r.iter().any(|m| {
            m == "Counter \"component_errors_total\" must include \"error_code\" to match error log."
        }));
    }

    #[test]
    fn validate_error_stage_must_be_constant() {
        let mut e = mk_event();
        e.logs = one_log("error", "Failed.", &["error_type", "stage"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "\"processing\""),
            ]),
        );
        let r = run("FooError", e);
        assert!(
            r.iter()
                .any(|m| m.contains("must be an \"error_stage\" constant"))
        );
    }

    #[test]
    fn validate_error_type_must_be_constant() {
        let mut e = mk_event();
        e.logs = one_log("error", "Failed.", &["error_type", "stage"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "\"connection_failed\""),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("FooError", e);
        assert!(
            r.iter()
                .any(|m| m.contains("must be an \"error_type\" constant"))
        );
    }

    #[test]
    fn validate_events_dropped_must_be_named_events_dropped() {
        let mut e = mk_event();
        e.logs = one_log(
            "error",
            "Events dropped.",
            &["count", "intentional", "reason"],
        );
        e.counters.insert(
            METRIC_NAME_EVENTS_DROPPED.to_string(),
            counter(&[("intentional", "false")]),
        );
        let r = run("BadlyNamed", e);
        assert!(
            r.iter()
                .any(|m| m == "EventsDropped events MUST be named \"___EventsDropped\".")
        );
    }

    #[test]
    fn validate_events_dropped_log_level_error_or_debug() {
        let mut e = mk_event();
        e.logs = one_log("info", "Dropped.", &["count", "intentional", "reason"]);
        e.counters.insert(
            METRIC_NAME_EVENTS_DROPPED.to_string(),
            counter(&[("intentional", "false")]),
        );
        let r = run("FooEventsDropped", e);
        assert!(
            r.iter().any(|m| {
                m.contains("MUST log with one of these levels: [\"error\", \"debug\"]")
            })
        );
    }

    #[test]
    fn validate_events_dropped_counter_required_and_excluded_tags() {
        let mut e = mk_event();
        e.logs = one_log("error", "Dropped.", &["count", "intentional", "reason"]);
        // Missing intentional, has reason and count (which it must NOT)
        e.counters.insert(
            METRIC_NAME_EVENTS_DROPPED.to_string(),
            counter(&[("reason", "\"r\""), ("count", "1")]),
        );
        let r = run("FooEventsDropped", e);
        assert!(r.iter().any(|m| {
            m == "Counter \"component_discarded_events_total\" MUST include tag \"intentional\"."
        }));
        assert!(r.iter().any(|m| {
            m == "Counter \"component_discarded_events_total\" MUST NOT include tag \"reason\"."
        }));
        assert!(r.iter().any(|m| {
            m == "Counter \"component_discarded_events_total\" MUST NOT include tag \"count\"."
        }));
    }

    #[test]
    fn validate_events_dropped_log_required_params() {
        let mut e = mk_event();
        // Error log present but missing required params (count, intentional, reason)
        e.logs = one_log("error", "Dropped.", &[]);
        e.counters.insert(
            METRIC_NAME_EVENTS_DROPPED.to_string(),
            counter(&[("intentional", "false")]),
        );
        let r = run("FooEventsDropped", e);
        for p in ["count", "intentional", "reason"] {
            assert!(
                r.iter().any(|m| m
                    == &format!(
                        "Error log for EventsDropped event MUST include parameter \"{p}\"."
                    )),
                "missing report for parameter {p} in: {r:?}"
            );
        }
    }

    #[test]
    fn validate_emits_dropped_must_not_also_increment_counter() {
        let mut e = mk_event();
        e.emits_component_events_dropped = true;
        e.counters.insert(
            METRIC_NAME_EVENTS_DROPPED.to_string(),
            counter(&[("intentional", "false")]),
        );
        let r = run("FooEventsDropped", e);
        assert!(r.iter().any(|m| {
            m.contains("should not also increment counter")
                && m.contains(METRIC_NAME_EVENTS_DROPPED)
        }));
    }

    #[test]
    fn validate_clean_event_no_reports() {
        // A correctly-shaped Error event should produce no reports.
        let mut e = mk_event();
        e.logs = one_log("error", "Connection failed.", &["error_type", "stage"]);
        e.counters.insert(
            METRIC_NAME_ERROR.to_string(),
            counter(&[
                ("error_type", "error_type::CONNECTION_FAILED"),
                ("stage", "error_stage::PROCESSING"),
            ]),
        );
        let r = run("ConnectionFailedError", e);
        assert!(r.is_empty(), "expected no reports, got: {r:?}");
    }
}
