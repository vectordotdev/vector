use std::collections::HashMap;

use convert_case::{Boundary, Case, Casing};
use once_cell::sync::OnceCell;

static WELL_KNOWN_REPLACEMENTS: OnceCell<HashMap<String, &'static str>> = OnceCell::new();

/// Methods for splitting an input string into word segments.
pub enum SplitMethod {
    /// Split inputs on case change boundaries.
    Case,

    /// Split inputs on underscore boundaries.
    Underscore,
}

/// Generates a human-friendly version of the given string.
///
/// Many instances exist where type names, or string constants, represent a condensed form of an
/// otherwise human-friendly/recognize string, such as "aws_s3" (for AWS S3) or "InfluxdbMetrics"
/// (for InfluxDB Metrics) and so on.
///
/// This function takes a given input and restores it back to the human-friendly version by
/// splitting it on the relevant word boundaries, adjusting the input to title case, and applying
/// well-known replacements to ensure that brand-specific casing (such as "CloudWatch" instead of
/// "Cloudwatch", or handling acronyms like AWS, GCP, and so on) makes it into the final version.
pub fn generate_human_friendly_version(input: &str, split: SplitMethod) -> String {
    // This specifically instructs the splitter to avoid treating a letter, followed by a
    // digit, as a word boundary. This matters for acronyms like "EC2" where they'll currently be
    // written out in code as "Ec2", as well as acronyms like "S3", where they're already set as
    // "S3" in code, and we want to keep them intact.
    let digit_boundaries = &[Boundary::LowerDigit, Boundary::UpperDigit];

    let normalized = match split {
        SplitMethod::Case => input
            .from_case(Case::Pascal)
            .without_boundaries(digit_boundaries)
            .to_case(Case::Title),
        SplitMethod::Underscore => input
            .from_case(Case::Snake)
            .without_boundaries(digit_boundaries)
            .to_case(Case::Title),
    };

    let replaced_segments = normalized
        .split(' ')
        .map(replace_well_known_segments)
        .collect::<Vec<_>>();
    replaced_segments.join(" ")
}

fn replace_well_known_segments(input: &str) -> String {
    let well_known_replacements = get_well_known_replacements_map();
    let well_known_acronyms = get_well_known_acronyms();

    let as_lower = input.to_lowercase();
    if let Some(replacement) = well_known_replacements.get(&as_lower) {
        replacement.to_string()
    } else if well_known_acronyms.contains(&as_lower.as_str()) {
        input.to_uppercase()
    } else {
        input.to_string()
    }
}

fn get_well_known_acronyms() -> &'static [&'static str] {
    &[
        "api", "amqp", "aws", "ec2", "ecs", "gcp", "hec", "http", "https", "nats", "nginx", "s3",
        "sqs", "tls", "ssl", "otel", "gelf", "csv", "json", "rfc3339", "lz4", "us", "eu", "bsd",
        "vrl", "tcp", "udp", "id", "uuid", "kms", "uri", "url", "acp", "uid", "ip", "pid",
        "ndjson", "ewma", "rtt", "cpu", "acl",
    ]
}

fn get_well_known_replacements_map() -> &'static HashMap<String, &'static str> {
    WELL_KNOWN_REPLACEMENTS.get_or_init(|| {
        let pairs = vec![
            ("eventstoredb", "EventStoreDB"),
            ("mongodb", "MongoDB"),
            ("opentelemetry", "OpenTelemetry"),
            ("otel", "OTEL"),
            ("postgresql", "PostgreSQL"),
            ("pubsub", "Pub/Sub"),
            ("statsd", "StatsD"),
            ("journald", "JournalD"),
            ("appsignal", "AppSignal"),
            ("clickhouse", "ClickHouse"),
            ("influxdb", "InfluxDB"),
            ("webhdfs", "WebHDFS"),
            ("cloudwatch", "CloudWatch"),
            ("logdna", "LogDNA"),
            ("geoip", "GeoIP"),
            ("ssekms", "SSE-KMS"),
            ("aes256", "AES-256"),
            ("apiserver", "API Server"),
            ("dir", "Directory"),
            ("ids", "IDs"),
            ("ips", "IPs"),
            ("grpc", "gRPC"),
            ("oauth2", "OAuth2"),
        ];

        pairs
            .iter()
            .map(|(k, v)| (k.to_lowercase(), *v))
            .collect()
    })
}
