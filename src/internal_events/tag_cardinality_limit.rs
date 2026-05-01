use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent},
};

#[derive(NamedInternalEvent)]
pub struct TagCardinalityLimitRejectingEvent<'a> {
    pub metric_name: &'a str,
    pub tag_key: &'a str,
    pub tag_value: &'a str,
    pub include_extended_tags: bool,
}

impl InternalEvent for TagCardinalityLimitRejectingEvent<'_> {
    fn emit(self) {
        debug!(
            message = "Event containing tag with new value after hitting configured 'value_limit'; discarding event.",
            metric_name = self.metric_name,
            tag_key = self.tag_key,
            tag_value = self.tag_value,
        );
        if self.include_extended_tags {
            counter!(
                CounterName::TagValueLimitExceededTotal,
                "metric_name" => self.metric_name.to_string(),
                "tag_key" => self.tag_key.to_string(),
            )
            .increment(1);
        } else {
            counter!(CounterName::TagValueLimitExceededTotal).increment(1);
        }

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Tag value limit exceeded."
        })
    }
}

#[derive(NamedInternalEvent)]
pub struct TagCardinalityLimitRejectingTag<'a> {
    pub metric_name: &'a str,
    pub tag_key: &'a str,
    pub tag_value: &'a str,
    pub include_extended_tags: bool,
}

impl InternalEvent for TagCardinalityLimitRejectingTag<'_> {
    fn emit(self) {
        debug!(
            message = "Rejecting tag after hitting configured 'value_limit'.",
            metric_name = self.metric_name,
            tag_key = self.tag_key,
            tag_value = self.tag_value,
        );
        if self.include_extended_tags {
            counter!(
                CounterName::TagValueLimitExceededTotal,
                "metric_name" => self.metric_name.to_string(),
                "tag_key" => self.tag_key.to_string(),
            )
            .increment(1);
        } else {
            counter!(CounterName::TagValueLimitExceededTotal).increment(1);
        }
    }
}

#[derive(NamedInternalEvent)]
pub struct TagCardinalityValueLimitReached<'a> {
    pub key: &'a str,
}

impl InternalEvent for TagCardinalityValueLimitReached<'_> {
    fn emit(self) {
        debug!(
            message = "Value_limit reached for key. New values for this key will be rejected.",
            key = %self.key,
        );
        counter!(CounterName::ValueLimitReachedTotal).increment(1);
    }
}
