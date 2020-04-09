use super::Transform;
use crate::{
    event::{self, Event},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use regex::RegexSet; // TODO: use regex::bytes
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SamplerConfig {
    pub rate: u64,
    pub key_field: Option<Atom>,
    #[serde(default)]
    pub pass_list: Vec<String>,
}

inventory::submit! {
    TransformDescription::new_without_default::<SamplerConfig>("sampler")
}

#[typetag::serde(name = "sampler")]
impl TransformConfig for SamplerConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(RegexSet::new(&self.pass_list)
            .map::<Box<dyn Transform>, _>(|regex_set| {
                Box::new(Sampler::new(self.rate, self.key_field.clone(), regex_set))
            })
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

pub struct Sampler {
    rate: u64,
    key_field: Atom,
    pass_list: RegexSet,
}

impl Sampler {
    pub fn new(rate: u64, key_field: Option<Atom>, pass_list: RegexSet) -> Self {
        let key_field = key_field.unwrap_or_else(|| event::log_schema().message_key().clone());
        Self {
            rate,
            key_field,
            pass_list,
        }
    }
}

impl Transform for Sampler {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let message = event
            .as_log()
            .get(&self.key_field)
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into());

        if self.pass_list.is_match(&message) {
            return Some(event);
        }

        if seahash::hash(message.as_bytes()) % self.rate == 0 {
            event
                .as_mut_log()
                .insert(Atom::from("sample_rate"), self.rate.to_string());

            Some(event)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sampler;
    use crate::event::{self, Event};
    use crate::transforms::Transform;
    use approx::assert_relative_eq;
    use regex::RegexSet;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn samples_at_roughly_the_configured_rate() {
        let num_events = 10000;

        let events = random_events(num_events);
        let mut sampler = Sampler::new(2, None, RegexSet::new(&["na"]).unwrap());
        let total_passed = events
            .into_iter()
            .filter_map(|event| sampler.transform(event))
            .count();
        let ideal = 1.0 as f64 / 2.0 as f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let events = random_events(num_events);
        let mut sampler = Sampler::new(25, None, RegexSet::new(&["na"]).unwrap());
        let total_passed = events
            .into_iter()
            .filter_map(|event| sampler.transform(event))
            .count();
        let ideal = 1.0 as f64 / 25.0 as f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);
    }

    #[test]
    fn consistely_samples_the_same_events() {
        let events = random_events(1000);
        let mut sampler = Sampler::new(2, None, RegexSet::new(&["na"]).unwrap());

        let first_run = events
            .clone()
            .into_iter()
            .filter_map(|event| sampler.transform(event))
            .collect::<Vec<_>>();
        let second_run = events
            .into_iter()
            .filter_map(|event| sampler.transform(event))
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_events_matching_pass_list() {
        let event = Event::from("i am important");
        let mut sampler = Sampler::new(0, None, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| sampler.transform(event.clone()))
            .count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn handles_key_field() {
        let event = Event::from("nananana");
        let mut sampler = Sampler::new(0, Some("timestamp".into()), RegexSet::new(&[":"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| sampler.transform(event.clone()))
            .count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn sampler_adds_sampling_rate_to_event() {
        let events = random_events(10000);
        let mut sampler = Sampler::new(10, None, RegexSet::new(&["na"]).unwrap());
        let passing = events
            .into_iter()
            .filter(|s| {
                !s.as_log()[&event::log_schema().message_key()]
                    .to_string_lossy()
                    .contains("na")
            })
            .find_map(|event| sampler.transform(event))
            .unwrap();
        assert_eq!(passing.as_log()[&Atom::from("sample_rate")], "10".into());

        let events = random_events(10000);
        let mut sampler = Sampler::new(25, None, RegexSet::new(&["na"]).unwrap());
        let passing = events
            .into_iter()
            .filter(|s| {
                !s.as_log()[&event::log_schema().message_key()]
                    .to_string_lossy()
                    .contains("na")
            })
            .find_map(|event| sampler.transform(event))
            .unwrap();
        assert_eq!(passing.as_log()[&Atom::from("sample_rate")], "25".into());

        // If the event passed the regex check, don't include the sampling rate
        let mut sampler = Sampler::new(25, None, RegexSet::new(&["na"]).unwrap());
        let event = Event::from("nananana");
        let passing = sampler.transform(event).unwrap();
        assert!(passing.as_log().get(&Atom::from("sample_rate")).is_none());
    }

    fn random_events(n: usize) -> Vec<Event> {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};

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
