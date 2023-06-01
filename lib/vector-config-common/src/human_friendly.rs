use std::collections::{HashMap, HashSet};

use convert_case::{Boundary, Case, Converter};
use once_cell::sync::Lazy;

/// Well-known replacements.
///
/// Replacements are instances of strings with unique capitalization that cannot be achieved
/// programmatically, as well as the potential insertion of additional characters, such as the
/// replacement of "pubsub" with "Pub/Sub".
static WELL_KNOWN_REPLACEMENTS: Lazy<HashMap<String, &'static str>> = Lazy::new(|| {
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

    pairs.iter().map(|(k, v)| (k.to_lowercase(), *v)).collect()
});

/// Well-known acronyms.
///
/// Acronyms are distinct from replacements because they should be entirely capitalized (i.e. "aws"
/// or "aWs" or "Aws" should always be replaced with "AWS") whereas replacements may insert
/// additional characters or capitalize specific characters within the original string.
static WELL_KNOWN_ACRONYMS: Lazy<HashSet<String>> = Lazy::new(|| {
    let acronyms = &[
        "api", "amqp", "aws", "ec2", "ecs", "gcp", "hec", "http", "https", "nats", "nginx", "s3",
        "sqs", "tls", "ssl", "otel", "gelf", "csv", "json", "rfc3339", "lz4", "us", "eu", "bsd",
        "vrl", "tcp", "udp", "id", "uuid", "kms", "uri", "url", "acp", "uid", "ip", "pid",
        "ndjson", "ewma", "rtt", "cpu", "acl", "imds", "acl", "alpn", "sasl",
    ];

    acronyms.iter().map(|s| s.to_lowercase()).collect()
});

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
pub fn generate_human_friendly_string(input: &str) -> String {
    // Create our case converter, which specifically ignores letter/digit boundaries, which is
    // important for not turning substrings like "Ec2" or "S3" into "Ec"/"2" and "S"/"3",
    // respectively.
    let converter = Converter::new()
        .to_case(Case::Title)
        .remove_boundaries(&[Boundary::LowerDigit, Boundary::UpperDigit]);
    let normalized = converter.convert(input);

    let replaced_segments = normalized
        .split(' ')
        .map(replace_well_known_segments)
        .collect::<Vec<_>>();
    replaced_segments.join(" ")
}

fn replace_well_known_segments(input: &str) -> String {
    let as_lower = input.to_lowercase();
    if let Some(replacement) = WELL_KNOWN_REPLACEMENTS.get(&as_lower) {
        replacement.to_string()
    } else if WELL_KNOWN_ACRONYMS.contains(&as_lower) {
        input.to_uppercase()
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::generate_human_friendly_string;

    #[test]
    fn autodetect_input_case() {
        let pascal_input = "LogToMetric";
        let snake_input = "log_to_metric";

        let pascal_friendly = generate_human_friendly_string(pascal_input);
        let snake_friendly = generate_human_friendly_string(snake_input);

        let expected = "Log To Metric";
        assert_eq!(expected, pascal_friendly);
        assert_eq!(expected, snake_friendly);
    }

    #[test]
    fn digit_letter_boundaries() {
        let input1 = "Ec2Metadata";
        let expected1 = "EC2 Metadata";
        let actual1 = generate_human_friendly_string(input1);
        assert_eq!(expected1, actual1);

        let input2 = "AwsS3";
        let expected2 = "AWS S3";
        let actual2 = generate_human_friendly_string(input2);
        assert_eq!(expected2, actual2);
    }
}
