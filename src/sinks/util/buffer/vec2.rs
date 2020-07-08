use super::{err_event_too_large, Batch, BatchSize, PushResult};

pub trait Length {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub struct VecBuffer2<T> {
    batch: Vec<T>,
    bytes: usize,
    settings: BatchSize,
}

impl<T> VecBuffer2<T> {
    pub fn new(settings: BatchSize) -> Self {
        Self::new_with_settings(settings)
    }

    fn new_with_settings(settings: BatchSize) -> Self {
        Self {
            batch: Vec::with_capacity(settings.events),
            bytes: 0,
            settings,
        }
    }
}

impl<T: Length> Batch for VecBuffer2<T> {
    type Input = T;
    type Output = Vec<T>;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let new_bytes = self.bytes + item.len();
        if self.is_empty() && item.len() > self.settings.bytes {
            err_event_too_large(item.len())
        } else if self.batch.len() >= self.settings.events || new_bytes > self.settings.bytes {
            PushResult::Overflow(item)
        } else {
            self.batch.push(item);
            self.bytes = new_bytes;
            PushResult::Ok(
                self.batch.len() >= self.settings.events || new_bytes >= self.settings.bytes,
            )
        }
    }

    fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new_with_settings(self.settings)
    }

    fn finish(self) -> Self::Output {
        self.batch
    }

    fn num_items(&self) -> usize {
        self.batch.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::BatchSettings;

    impl Length for String {
        fn len(&self) -> usize {
            self.len() + 1
        }
    }

    #[test]
    fn obeys_max_events() {
        let settings = BatchSettings::default().events(2).size;
        let mut buffer = VecBuffer2::new(settings);
        let data = "dummy".to_string();

        assert_eq!(buffer.is_empty(), true);
        assert_eq!(buffer.num_items(), 0);

        assert_eq!(buffer.push(data.clone()), PushResult::Ok(false));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 1);

        assert_eq!(buffer.push(data.clone()), PushResult::Ok(true));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 2);

        assert_eq!(buffer.push(data.clone()), PushResult::Overflow(data));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 2);

        assert_eq!(buffer.finish().len(), 2);
    }

    #[test]
    fn obeys_max_bytes() {
        let settings = BatchSettings::default().events(99).bytes(22).size;
        let mut buffer = VecBuffer2::new(settings);
        let data = "some bytes".to_string();

        assert_eq!(buffer.is_empty(), true);
        assert_eq!(buffer.num_items(), 0);

        assert_eq!(
            buffer.push("this record is just too long to be inserted".into()),
            PushResult::Ok(false)
        );
        assert_eq!(buffer.is_empty(), true);
        assert_eq!(buffer.num_items(), 0);

        assert_eq!(buffer.push(data.clone()), PushResult::Ok(false));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 1);

        assert_eq!(buffer.push(data.clone()), PushResult::Ok(true));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 2);

        assert_eq!(buffer.push(data.clone()), PushResult::Overflow(data));
        assert_eq!(buffer.is_empty(), false);
        assert_eq!(buffer.num_items(), 2);

        assert_eq!(buffer.finish().len(), 2);
    }
}
