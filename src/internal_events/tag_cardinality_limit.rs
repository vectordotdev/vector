use vector_lib::{
    NamedInternalEvent, counter, gauge,
    internal_event::{
        ComponentEventsDropped, CounterName, GaugeName, INTENTIONAL, InternalEvent, LossyF64,
    },
};

#[derive(NamedInternalEvent)]
pub(crate) struct TagCardinalityLimitRejectingEvent<'a> {
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
        });
    }
}

#[derive(NamedInternalEvent)]
pub(crate) struct TagCardinalityLimitRejectingTag<'a> {
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
pub(crate) struct TagCardinalityValueLimitReached<'a> {
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

#[derive(NamedInternalEvent)]
pub(crate) struct TagCardinalityLimitUntracked;

impl InternalEvent for TagCardinalityLimitUntracked {
    fn emit(self) {
        debug!(
            message = "Max tracked keys limit reached; forwarding one or more metric tags without cardinality checks."
        );
        counter!(CounterName::TagCardinalityUntrackedEventsTotal).increment(1);
    }
}

#[derive(NamedInternalEvent)]
pub(crate) struct TagCardinalityTrackedKeys {
    pub count: usize,
}

impl InternalEvent for TagCardinalityTrackedKeys {
    fn emit(self) {
        gauge!(GaugeName::TagCardinalityTrackedKeys).set(LossyF64(self.count));
    }
}

/// Emitted when TTL eviction reclaims tag values from a tracking bucket.
///
/// The `count` is in cache *slots*, the unit `AcceptedTagValueSet::len`
/// measures when enforcing `value_limit`. For the exact backend a slot holds
/// one distinct value, so the count is exact. For the probabilistic backend it
/// is an approximation of the distinct values that aged out of the retired
/// rolling-bloom shard, not a bound. Refresh-on-sighting can over-count,
/// because a hot value copied into a newer shard is still included when the
/// old shard rotates out. A bloom false positive can under-count, because
/// `BloomFilterStorage::insert` leaves the shard count unchanged when the
/// filter already reports the value as present.
#[derive(NamedInternalEvent)]
pub(crate) struct TagCardinalityTtlExpired {
    pub count: u64,
}

impl InternalEvent for TagCardinalityTtlExpired {
    fn emit(self) {
        if self.count == 0 {
            return;
        }
        debug!(
            message = "Expired tag values from cardinality cache.",
            count = self.count,
        );
        counter!(CounterName::TagCardinalityTtlExpirationsTotal).increment(self.count);
    }
}
