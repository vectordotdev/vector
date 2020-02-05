use chrono::{DateTime, Utc};

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp_nanos()
    } else {
        encode_timestamp(Some(Utc::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    #[test]
    fn test_encode_timestamp() {
        let start = Utc::now().timestamp_nanos();
        assert_eq!(encode_timestamp(Some(ts())), 1542182950000000011);
        assert!(encode_timestamp(None) >= start)
    }
}