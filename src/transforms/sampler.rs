use super::Transform;
use crate::event::{self, Event};
use regex::RegexSet; // TODO: use regex::bytes
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SamplerConfig {
    pub rate: u64,
    pub pass_list: Vec<String>,
}

#[typetag::serde(name = "sampler")]
impl crate::topology::config::TransformConfig for SamplerConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        RegexSet::new(&self.pass_list)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|regex_set| Box::new(Sampler::new(self.rate, regex_set)))
    }
}

pub struct Sampler {
    rate: u64,
    pass_list: RegexSet,
}

impl Sampler {
    pub fn new(rate: u64, pass_list: RegexSet) -> Self {
        Self { rate, pass_list }
    }
}

impl Transform for Sampler {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let message = event.as_log()[&event::MESSAGE].to_string_lossy();

        if self.pass_list.is_match(&message) {
            return Some(event);
        }

        if seahash::hash(message.as_bytes()) % self.rate == 0 {
            event
                .as_mut_log()
                .insert_implicit(Atom::from("sample_rate"), self.rate.to_string().into());

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
        let sampler = Sampler::new(2, RegexSet::new(&["na"]).unwrap());
        let total_passed = events
            .into_iter()
            .filter_map(|event| sampler.transform(event))
            .count();
        let ideal = 1.0 as f64 / 2.0 as f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let events = random_events(num_events);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
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
        let sampler = Sampler::new(2, RegexSet::new(&["na"]).unwrap());

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
        let sampler = Sampler::new(0, RegexSet::new(&["important"]).unwrap());
        let iterations = 0..1000;
        let total_passed = iterations
            .filter_map(|_| sampler.transform(event.clone()))
            .count();
        assert_eq!(total_passed, 1000);
    }

    #[test]
    fn sampler_adds_sampling_rate_to_event() {
        let events = random_events(10000);
        let sampler = Sampler::new(10, RegexSet::new(&["na"]).unwrap());
        let passing = events
            .into_iter()
            .filter(|s| !s.as_log()[&event::MESSAGE].to_string_lossy().contains("na"))
            .find_map(|event| sampler.transform(event))
            .unwrap();
        assert_eq!(passing.as_log()[&Atom::from("sample_rate")], "10".into());

        let events = random_events(10000);
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
        let passing = events
            .into_iter()
            .filter(|s| !s.as_log()[&event::MESSAGE].to_string_lossy().contains("na"))
            .find_map(|event| sampler.transform(event))
            .unwrap();
        assert_eq!(passing.as_log()[&Atom::from("sample_rate")], "25".into());

        // If the event passed the regex check, don't include the sampling rate
        let sampler = Sampler::new(25, RegexSet::new(&["na"]).unwrap());
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
