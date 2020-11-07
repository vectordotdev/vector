use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
    internal_events::{SamplerEventDiscarded, SamplerEventProcessed},
    transforms::{FunctionTransform, Transform},
};
use regex::RegexSet; // TODO: use regex::bytes
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SampleProperty {
    Hash,
    Index,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SamplerConfig {
    pub rate: u64,
    pub key_field: Option<String>,
    #[serde(default)]
    pub pass_list: Vec<String>,
    #[serde(default = "default_property")]
    pub property: SampleProperty,
}

inventory::submit! {
    TransformDescription::new::<SamplerConfig>("sampler")
}

impl GenerateConfig for SamplerConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            rate: 10,
            key_field: None,
            pass_list: Vec::new(),
            property: default_property(),
        })
        .unwrap()
    }
}

fn default_property() -> SampleProperty {
    SampleProperty::Index
}

#[async_trait::async_trait]
#[typetag::serde(name = "sampler")]
impl TransformConfig for SamplerConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(RegexSet::new(&self.pass_list)
            .map(|regex_set| {
                Sampler::new(self.rate, self.key_field.clone(), regex_set, self.property)
            })
            .map(Transform::function)
            .context(super::InvalidRegex)?)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "sampler"
    }
}

#[derive(Clone, Debug)]
pub struct Sampler {
    rate: u64,
    key_field: Option<String>,
    pass_list: RegexSet,
    property: SampleProperty,
    count: u64,
}

impl Sampler {
    pub fn new(
        rate: u64,
        key_field: Option<String>,
        pass_list: RegexSet,
        property: SampleProperty,
    ) -> Self {
        Self {
            rate,
            key_field,
            pass_list,
            property,
            count: 0,
        }
    }

    fn fetch_and_increment(&mut self) -> u64 {
        let value = self.count;
        self.count = (value + 1) % self.rate;
        value
    }
}

impl FunctionTransform for Sampler {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let message = self
            .key_field
            .as_ref()
            .and_then(|key_field| event.as_log().get(key_field))
            .map(|v| v.to_string_lossy());

        emit!(SamplerEventProcessed);

        let num = match (self.property, message) {
            (_, Some(ref message)) if self.pass_list.is_match(message) => None,
            (SampleProperty::Index, _) | (_, None) => Some(self.fetch_and_increment()),
            (SampleProperty::Hash, Some(message)) => Some(seahash::hash(message.as_bytes())),
        };

        match num {
            None => output.push(event),
            Some(num) if num % self.rate == 0 => {
                event
                    .as_mut_log()
                    .insert("sample_rate", self.rate.to_string());
                output.push(event);
            }
            _ => emit!(SamplerEventDiscarded),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;
    use crate::event::Event;
    use approx::assert_relative_eq;
    use regex::RegexSet;

    #[test]
    fn genreate_config() {
        crate::test_util::test_generate_config::<SamplerConfig>();
    }

    #[test]
    fn hash_samples_at_roughly_the_configured_rate() {
        let num_events = 10000;

        let events = random_events(num_events);
        let mut sampler = Sampler::new(
            2,
            Some(log_schema().message_key().into()),
            RegexSet::new(&["na"]).unwrap(),
            SampleProperty::Hash,
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| sampler.transform_one(event))
            .count();
        let ideal = 1.0 as f64 / 2.0 as f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let events = random_events(num_events);
        let mut sampler = Sampler::new(
            25,
            Some(log_schema().message_key().into()),
            RegexSet::new(&["na"]).unwrap(),
            SampleProperty::Hash,
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| sampler.transform_one(event))
            .count();
        let ideal = 1.0 as f64 / 25.0 as f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);
    }

    #[test]
    fn hash_consistently_samples_the_same_events() {
        let events = random_events(1000);
        let mut sampler = Sampler::new(
            2,
            Some(log_schema().message_key().into()),
            RegexSet::new(&["na"]).unwrap(),
            SampleProperty::Hash,
        );

        let first_run = events
            .clone()
            .into_iter()
            .filter_map(|event| sampler.transform_one(event))
            .collect::<Vec<_>>();
        let second_run = events
            .into_iter()
            .filter_map(|event| sampler.transform_one(event))
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_events_matching_pass_list() {
        for mode in vec![SampleProperty::Index, SampleProperty::Hash] {
            let event = Event::from("i am important");
            let mut sampler = Sampler::new(
                0,
                Some(log_schema().message_key().into()),
                RegexSet::new(&["important"]).unwrap(),
                mode,
            );
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| sampler.transform_one(event.clone()))
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn handles_key_field() {
        for mode in vec![SampleProperty::Index, SampleProperty::Hash] {
            let event = Event::from("nananana");
            let mut sampler = Sampler::new(
                0,
                Some("timestamp".into()),
                RegexSet::new(&[":"]).unwrap(),
                mode,
            );
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| sampler.transform_one(event.clone()))
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn sampler_adds_sampling_rate_to_event() {
        for mode in vec![SampleProperty::Index, SampleProperty::Hash] {
            let events = random_events(10000);
            let mut sampler = Sampler::new(
                10,
                Some(log_schema().message_key().into()),
                RegexSet::new(&["na"]).unwrap(),
                mode,
            );
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| sampler.transform_one(event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "10".into());

            let events = random_events(10000);
            let mut sampler = Sampler::new(
                25,
                Some(log_schema().message_key().into()),
                RegexSet::new(&["na"]).unwrap(),
                mode,
            );
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| sampler.transform_one(event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "25".into());

            // If the event passed the regex check, don't include the sampling rate
            let mut sampler = Sampler::new(
                25,
                Some(log_schema().message_key().into()),
                RegexSet::new(&["na"]).unwrap(),
                mode,
            );
            let event = Event::from("nananana");
            let passing = sampler.transform_one(event).unwrap();
            assert!(passing.as_log().get("sample_rate").is_none());
        }
    }

    fn random_events(n: usize) -> Vec<Event> {
        use rand::{thread_rng, Rng};
        use rand_distr::Alphanumeric;

        (0..n)
            .map(|_| {
                thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(10)
                    .collect::<String>()
            })
            .map(Event::from)
            .collect()
    }
}
