use super::{
    err_event_too_large, Batch, BatchConfig, BatchError, BatchMaker, BatchSettings, BatchSize,
    PushResult,
};
use bytes::Bytes;

pub trait EncodedLength {
    fn encoded_length(&self) -> usize;
}

#[derive(Clone)]
pub struct VecBuffer<T> {
    batch: Vec<T>,
    bytes: usize,
    settings: BatchSize<Self>,
}

pub struct VecBufferMaker<T> {
    settings: BatchSize<VecBuffer<T>>,
}

impl<T: EncodedLength> BatchMaker for VecBufferMaker<T> {
    type Batch = VecBuffer<T>;
    fn new_batch(&self) -> Self::Batch {
        Self::Batch::with_settings(self.settings)
    }
}

impl<T> VecBuffer<T> {
    fn with_settings(settings: BatchSize<Self>) -> Self {
        Self {
            batch: Vec::with_capacity(settings.events),
            bytes: 0,
            settings,
        }
    }

    pub fn maker(settings: BatchSize<Self>) -> VecBufferMaker<T> {
        VecBufferMaker { settings }
    }
}

impl<T: EncodedLength> Batch for VecBuffer<T> {
    type Input = T;
    type Output = Vec<T>;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(config
            .use_size_as_events()?
            .get_settings_or_default(defaults))
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let new_bytes = self.bytes + item.encoded_length();
        if self.is_empty() && item.encoded_length() > self.settings.bytes {
            err_event_too_large(item.encoded_length())
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

    fn finish(self) -> Self::Output {
        self.batch
    }

    fn num_items(&self) -> usize {
        self.batch.len()
    }
}

impl EncodedLength for Bytes {
    fn encoded_length(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::BatchSettings;

    impl EncodedLength for String {
        fn encoded_length(&self) -> usize {
            self.len() + 1
        }
    }

    #[test]
    fn obeys_max_events() {
        let settings = BatchSettings::default().events(2).size;
        let mut buffer = VecBuffer::maker(settings).new_batch();
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
        let mut buffer = VecBuffer::maker(settings).new_batch();
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
