use std::{collections::HashSet, fmt, net::IpAddr, str::FromStr};

use cidr::IpCidr;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
    ser::SerializeSeq,
};
use vector_config::Configurable;

/// Hosts and networks that should be contacted without using a proxy.
#[derive(Clone, Configurable, Debug, Default, Deserialize, Eq, PartialEq)]
// Derive the schema from the input forms while retaining the visitor and streaming serializer.
#[serde(from = "NoProxyRepr", into = "NoProxyRepr")]
pub struct NoProxy {
    patterns: HashSet<Pattern>,
    has_wildcard: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Pattern {
    Wildcard,
    IpCidr {
        source: String,
        network: IpCidr,
    },
    Host {
        source: String,
        match_kind: HostMatch,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum HostMatch {
    /// The pattern has no leading or trailing dot.
    Exact,
    /// The pattern ends with a dot.
    Prefix,
    /// The pattern starts with a dot.
    Suffix,
    /// The pattern starts and ends with a dot.
    Contains,
}

impl Pattern {
    fn parse(value: &str) -> Option<Self> {
        let source = value.trim();
        if source.is_empty() {
            return None;
        }

        if source == "*" {
            return Some(Self::Wildcard);
        }

        if let Ok(network) = IpCidr::from_str(source) {
            return Some(Self::IpCidr {
                source: source.to_owned(),
                network,
            });
        }

        let match_kind = match (source.starts_with('.'), source.ends_with('.')) {
            (false, false) => HostMatch::Exact,
            (false, true) => HostMatch::Prefix,
            (true, false) => HostMatch::Suffix,
            (true, true) => HostMatch::Contains,
        };
        Some(Self::Host {
            source: source.to_owned(),
            match_kind,
        })
    }

    fn matches(&self, candidate: &str) -> bool {
        let candidate = strip_ipv6_brackets(candidate);
        match self {
            Self::Wildcard => true,
            Self::IpCidr { source, network } => {
                candidate == source
                    || candidate
                        .parse::<IpAddr>()
                        .is_ok_and(|candidate| network.contains(&candidate))
            }
            Self::Host { source, match_kind } => match match_kind {
                HostMatch::Exact => candidate == source,
                HostMatch::Prefix => candidate.starts_with(source),
                HostMatch::Suffix => candidate.ends_with(source),
                HostMatch::Contains => candidate.contains(source),
            },
        }
    }

    fn source(&self) -> &str {
        match self {
            Self::Wildcard => "*",
            Self::IpCidr { source, .. } | Self::Host { source, .. } => source,
        }
    }
}

fn strip_ipv6_brackets(value: &str) -> &str {
    value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value)
}

impl NoProxy {
    fn from_entries(entries: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let patterns: HashSet<_> = entries
            .into_iter()
            .filter_map(|entry| Pattern::parse(entry.as_ref()))
            .collect();
        let has_wildcard = patterns.contains(&Pattern::Wildcard);

        Self {
            patterns,
            has_wildcard,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn matches(&self, candidate: &str) -> bool {
        if self.has_wildcard {
            return true;
        }

        self.patterns
            .iter()
            .any(|pattern| pattern.matches(candidate))
    }
}

impl From<&str> for NoProxy {
    fn from(value: &str) -> Self {
        Self::from_entries(value.split(','))
    }
}

impl From<String> for NoProxy {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl fmt::Display for NoProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, pattern) in self.patterns.iter().enumerate() {
            if index > 0 {
                formatter.write_str(",")?;
            }
            formatter.write_str(pattern.source())?;
        }
        Ok(())
    }
}

impl Serialize for NoProxy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.patterns.len()))?;
        for pattern in &self.patterns {
            sequence.serialize_element(pattern.source())?;
        }
        sequence.end()
    }
}

/// Hosts and networks that should be contacted without using a proxy.
#[derive(Configurable, Serialize)]
#[serde(untagged)]
enum NoProxyRepr {
    /// Comma-separated bypass patterns.
    // The docs resolver collapses string-or-array schemas to strings, which conflicts with
    // the [] default. Keep documenting the list form while accepting both input forms.
    #[configurable(metadata(docs::hidden))]
    String(String),
    /// Individual bypass patterns, without splitting entries on commas.
    Array(Vec<String>),
}

struct NoProxyVisitor;

impl<'de> Visitor<'de> for NoProxyVisitor {
    type Value = NoProxyRepr;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a comma-separated string or a list of strings")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(NoProxyRepr::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(NoProxyRepr::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut entries = Vec::new();
        while let Some(entry) = sequence.next_element::<String>()? {
            entries.push(entry);
        }
        Ok(NoProxyRepr::Array(entries))
    }
}

impl<'de> Deserialize<'de> for NoProxyRepr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(NoProxyVisitor)
    }
}

impl From<NoProxyRepr> for NoProxy {
    fn from(value: NoProxyRepr) -> Self {
        match value {
            NoProxyRepr::String(value) => Self::from(value),
            NoProxyRepr::Array(entries) => Self::from_entries(entries),
        }
    }
}

impl From<NoProxy> for NoProxyRepr {
    fn from(value: NoProxy) -> Self {
        Self::Array(
            value
                .patterns
                .into_iter()
                .map(|pattern| match pattern {
                    Pattern::Wildcard => "*".to_owned(),
                    Pattern::IpCidr { source, .. } | Pattern::Host { source, .. } => source,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

    use crate::config::proxy::ProxyConfig;

    #[test]
    fn matches_supported_patterns() {
        let cases = [
            ("*", "anything.example", true),
            ("example.com", "example.com", true),
            ("example.com", "api.example.com", false),
            (".example.com", "example.com", false),
            (".example.com", "api.example.com", true),
            (".example.com", "notexample.com", false),
            ("example.", "example.com", true),
            ("example.", "api.example.com", false),
            (".example.", "api.example.com", true),
            ("example.com:8080", "example.com:8080", true),
            ("example.com:8080", "example.com:8081", false),
            ("127.0.0.1", "127.0.0.1", true),
            ("127.0.0.1", "127.0.0.2", false),
            ("192.168.0.0/16", "192.168.42.1", true),
            ("192.168.0.0/16", "192.169.0.1", false),
            ("192.168.0.0/016", "192.168.42.1", true),
            ("192.168.0.0/016", "192.169.0.1", false),
            ("192.168.0.0/16", "192.168.0.0/16", true),
            ("192.168.1.1/24", "192.168.1.2", false),
            ("192.168.1.1/33", "192.168.1.1/33", true),
            ("2001:db8::1", "[2001:db8::1]", true),
            ("[2001:db8::1]", "[2001:db8::1]", false),
            ("2001:db8::/32", "[2001:db8:1::1]", true),
            ("2001:db8::/32", "2001:db9::1", false),
            ("2001:db8::/0032", "[2001:db8:1::1]", true),
            ("2001:db8::/0032", "2001:db9::1", false),
        ];

        for (pattern, candidate, expected) in cases {
            assert_eq!(
                NoProxy::from(pattern).matches(candidate),
                expected,
                "pattern {pattern:?} against {candidate:?}"
            );
        }
    }

    #[test]
    fn ignores_empty_entries_and_whitespace() {
        let no_proxy = NoProxy::from(" , localhost, , 127.0.0.1 ,");

        assert!(no_proxy.matches("localhost"));
        assert!(no_proxy.matches("127.0.0.1"));
        assert!(!no_proxy.matches("example.com"));
    }

    #[test]
    fn deserializes_from_string_or_list() {
        let from_string: NoProxy = serde_json::from_str(r#""localhost,127.0.0.1""#).unwrap();
        let from_list: NoProxy = serde_json::from_str(r#"["localhost", "127.0.0.1"]"#).unwrap();

        assert_eq!(from_string, from_list);
    }

    #[test]
    fn list_entries_are_not_split_on_commas() {
        let no_proxy: NoProxy = serde_json::from_value(json!(["localhost,127.0.0.1"])).unwrap();

        assert!(no_proxy.matches("localhost,127.0.0.1"));
        assert!(!no_proxy.matches("localhost"));
        assert!(!no_proxy.matches("127.0.0.1"));
    }

    #[test]
    fn rejects_invalid_inputs_with_existing_errors() {
        for (input, expected) in [
            (
                json!(true),
                "invalid type: boolean `true`, expected a comma-separated string or a list of strings",
            ),
            (
                json!(null),
                "invalid type: null, expected a comma-separated string or a list of strings",
            ),
            (
                json!({}),
                "invalid type: map, expected a comma-separated string or a list of strings",
            ),
            (json!([42]), "invalid type: integer `42`, expected a string"),
        ] {
            assert_eq!(
                serde_json::from_value::<NoProxy>(input)
                    .unwrap_err()
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    fn representation_matches_streaming_serializer() {
        for input in [
            json!("localhost,127.0.0.1"),
            json!(["localhost", "127.0.0.1"]),
            json!(["localhost,127.0.0.1"]),
            json!(["localhost", "localhost", " "]),
            json!(["*", "192.168.0.0/16"]),
            json!([]),
        ] {
            let no_proxy: NoProxy = serde_json::from_value(input).unwrap();

            let expected = serde_json::to_value(&no_proxy).unwrap();
            let actual = serde_json::to_value(NoProxyRepr::from(no_proxy)).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn schema_describes_both_input_forms() {
        let schema = serde_json::to_value(generate_root_schema::<ProxyConfig>().unwrap()).unwrap();
        let no_proxy = &schema["properties"]["no_proxy"];
        let variants = no_proxy["oneOf"].as_array().unwrap();

        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0]["type"], "string");
        assert_eq!(variants[1]["type"], "array");
        assert_eq!(variants[1]["items"]["type"], "string");
        assert_eq!(no_proxy["_metadata"]["docs::enum_tagging"], "untagged");
        // Continue documenting the list form and its default.
        assert_eq!(variants[0]["_metadata"]["docs::hidden"], true);
        assert_eq!(no_proxy["default"], json!([]));
    }

    #[test]
    fn serializes_as_a_deduplicated_list() {
        let no_proxy = NoProxy::from("localhost,localhost,127.0.0.1");
        let mut serialized = serde_json::to_value(no_proxy)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        serialized.sort();

        assert_eq!(serialized, ["127.0.0.1", "localhost"]);
    }
}
