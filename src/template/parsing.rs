use super::*;

/// One part of the template string after parsing.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Part {
    /// A literal piece of text to be copied verbatim into the output.
    Literal(String),
    /// A reference to the source event, to be copied from the relevant field or tag.
    Reference(String),
}

pub(super) fn parse_literal(src: &str) -> Part {
    Part::Literal(src.to_string())
}

// Pre-parse the template string into a series of parts to be filled in at render time.
pub(super) fn parse_template(src: &str) -> Result<Vec<Part>, TemplateParseError> {
    let mut last_end = 0;
    let mut parts = Vec::new();
    for cap in RE.captures_iter(src) {
        let all = cap.get(0).expect("Capture 0 is always defined");
        if all.start() > last_end {
            #[expect(
                clippy::string_slice,
                reason = "indices come from regex match positions, always char boundaries"
            )]
            parts.push(parse_literal(&src[last_end..all.start()]));
        }

        let path = cap[1].trim().to_owned();

        // This checks the syntax, but doesn't yet store it for use later
        // see: https://github.com/vectordotdev/vector/issues/14864
        if parse_target_path(&path).is_err() {
            return Err(TemplateParseError::InvalidPathSyntax { path });
        }

        parts.push(Part::Reference(path));
        last_end = all.end();
    }
    if src.len() > last_end {
        #[expect(
            clippy::string_slice,
            reason = "last_end comes from a regex match end position, always a char boundary"
        )]
        parts.push(parse_literal(&src[last_end..]));
    }

    Ok(parts)
}

pub(super) fn render_metric_field<'a>(key: &str, metric: &'a Metric) -> Option<&'a str> {
    match key {
        "name" => Some(metric.name()),
        "namespace" => metric.namespace(),
        _ if let Some(tag_key) = key.strip_prefix("tags.") => {
            metric.tags().and_then(|tags| tags.get(tag_key))
        }
        _ => None,
    }
}
