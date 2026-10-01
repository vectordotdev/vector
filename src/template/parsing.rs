use super::*;

/// One part of the template string after parsing.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Part {
    /// A literal piece of text to be copied verbatim into the output.
    Literal(String),
    /// A literal piece of text containing a time format string.
    Strftime(ParsedStrftime),
    /// A reference to the source event, to be copied from the relevant field or tag.
    Reference(String),
}

// Wrap the parsed time formatter in order to provide `impl Hash` and some convenience functions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ParsedStrftime(Box<[Item<'static>]>);

impl ParsedStrftime {
    fn parse(fmt: &str) -> Result<Self, TemplateParseError> {
        Ok(Self(
            StrftimeItems::new(fmt)
                .map(|item| match item {
                    // Box the references so they outlive the reference
                    Item::Space(space) => Item::OwnedSpace(space.into()),
                    Item::Literal(lit) => Item::OwnedLiteral(lit.into()),
                    // And copy all the others
                    Item::Fixed(f) => Item::Fixed(f),
                    Item::Numeric(num, pad) => Item::Numeric(num, pad),
                    Item::Error => Item::Error,
                    Item::OwnedSpace(space) => Item::OwnedSpace(space),
                    Item::OwnedLiteral(lit) => Item::OwnedLiteral(lit),
                })
                .map(|item| {
                    matches!(item, Item::Error)
                        .then(|| Err(TemplateParseError::StrftimeError))
                        .unwrap_or(Ok(item))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ))
    }

    fn is_dynamic(&self) -> bool {
        self.0.iter().any(|item| match item {
            Item::Fixed(_) => true,
            Item::Numeric(_, _) => true,
            Item::Error
            | Item::Space(_)
            | Item::OwnedSpace(_)
            | Item::Literal(_)
            | Item::OwnedLiteral(_) => false,
        })
    }

    fn as_items(&self) -> impl Iterator<Item = &Item<'static>> + Clone {
        self.0.iter()
    }

    pub(super) fn reserve_size(&self) -> usize {
        self.0
            .iter()
            .map(|item| match item {
                Item::Literal(lit) => lit.len(),
                Item::OwnedLiteral(lit) => lit.len(),
                Item::Space(space) => space.len(),
                Item::OwnedSpace(space) => space.len(),
                Item::Error => 0,
                Item::Numeric(_, _) => 2,
                Item::Fixed(_) => 2,
            })
            .sum()
    }
}

pub(super) fn parse_literal(src: &str) -> Result<Part, TemplateParseError> {
    let parsed = ParsedStrftime::parse(src)?;
    Ok(if parsed.is_dynamic() {
        Part::Strftime(parsed)
    } else {
        Part::Literal(src.to_string())
    })
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
            parts.push(parse_literal(&src[last_end..all.start()])?);
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
        parts.push(parse_literal(&src[last_end..])?);
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

pub(super) fn render_timestamp(
    items: &ParsedStrftime,
    event: EventRef<'_>,
    tz_offset: Option<FixedOffset>,
) -> String {
    let timestamp = match event {
        EventRef::Log(log) => log.get_timestamp().and_then(Value::as_timestamp).copied(),
        EventRef::Metric(metric) => metric.timestamp(),
        EventRef::Trace(trace) => {
            log_schema()
                .timestamp_key_target_path()
                .and_then(|timestamp_key| {
                    trace
                        .get(timestamp_key)
                        .and_then(Value::as_timestamp)
                        .copied()
                })
        }
    }
    .unwrap_or_else(Utc::now);

    match tz_offset {
        Some(offset) => timestamp
            .with_timezone(&offset)
            .format_with_items(items.as_items())
            .to_string(),
        None => timestamp
            .with_timezone(&chrono::Utc)
            .format_with_items(items.as_items())
            .to_string(),
    }
}
