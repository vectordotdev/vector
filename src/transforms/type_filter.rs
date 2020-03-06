use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TypeFilterConfig {
    pub filter_type: FilterType,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    Log,
    Metric,
}

inventory::submit! {
    TransformDescription::new_without_default::<TypeFilterConfig>("type_filter")
}

#[typetag::serde(name = "type_filter")]
impl TransformConfig for TypeFilterConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(TypeFilter::new(self.filter_type)))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        match self.filter_type {
            FilterType::Log => DataType::Log,
            FilterType::Metric => DataType::Metric,
        }
    }

    fn transform_type(&self) -> &'static str {
        "type_filter"
    }
}

pub struct TypeFilter {
    filter_type: FilterType,
}

impl TypeFilter {
    pub fn new(filter_type: FilterType) -> Self {
        Self { filter_type }
    }
}

impl Transform for TypeFilter {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match (&event, self.filter_type) {
            (Event::Log(_), FilterType::Log) => Some(event),
            (Event::Log(_), FilterType::Metric) => None,
            (Event::Metric(_), FilterType::Log) => None,
            (Event::Metric(_), FilterType::Metric) => Some(event),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{FilterType, TypeFilter};
    use crate::{transforms::Transform, Event};

    #[test]
    fn filters_based_on_type() {
        let log = Event::new_empty_log();
        let metric = Event::new_empty_counter();
        let mut log_filter = TypeFilter::new(FilterType::Log);
        let mut metric_filter = TypeFilter::new(FilterType::Metric);

        assert_eq!(Some(log.clone()), log_filter.transform(log.clone()));
        assert_eq!(
            Some(metric.clone()),
            metric_filter.transform(metric.clone())
        );
        assert_eq!(None, log_filter.transform(metric));
        assert_eq!(None, metric_filter.transform(log));
    }
}
