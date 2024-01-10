use metrics::counter;
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

pub struct TagCardinalityLimitRejectingEvent<'a> {
    pub metric_name: &'a str,
    pub tag_key: &'a str,
    pub tag_value: &'a str,
}

impl<'a> InternalEvent for TagCardinalityLimitRejectingEvent<'a> {
    fn emit(self) {
        debug!(
            message = "Event containing tag with new value after hitting configured 'value_limit'; discarding event.",
            metric_name = self.metric_name,
            tag_key = self.tag_key,
            tag_value = self.tag_value,
            internal_log_rate_limit = true,
        );
        counter!("tag_value_limit_exceeded_total", 1);

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Tag value limit exceeded."
        })
    }
}

pub struct TagCardinalityLimitRejectingTag<'a> {
    pub metric_name: &'a str,
    pub tag_key: &'a str,
    pub tag_value: &'a str,
}

impl<'a> InternalEvent for TagCardinalityLimitRejectingTag<'a> {
    fn emit(self) {
        debug!(
            message = "Rejecting tag after hitting configured 'value_limit'.",
            metric_name = self.metric_name,
            tag_key = self.tag_key,
            tag_value = self.tag_value,
            internal_log_rate_limit = true,
        );
        counter!("tag_value_limit_exceeded_total", 1);
    }
}

pub struct TagCardinalityValueLimitReached<'a> {
    pub key: &'a str,
}

impl<'a> InternalEvent for TagCardinalityValueLimitReached<'a> {
    fn emit(self) {
        debug!(
            message = "Value_limit reached for key. New values for this key will be rejected.",
            key = %self.key,
        );
        counter!("value_limit_reached_total", 1);
    }
}
