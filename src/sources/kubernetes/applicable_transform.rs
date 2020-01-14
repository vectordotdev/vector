use crate::{event::Event, transforms::Transform};

/// Contains several transforms. On the first message, transforms are tried
/// out one after the other until the first successful one has been found.
/// After that the transform will always be used.
///
/// If nothing succeds the message is still passed.
pub enum ApplicableTransform {
    Candidates(Vec<Box<dyn Transform>>),
    Transform(Option<Box<dyn Transform>>),
}

impl Transform for ApplicableTransform {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self {
            Self::Candidates(candidates) => {
                let candidate = candidates
                    .iter_mut()
                    .enumerate()
                    .find_map(|(i, t)| t.transform(event.clone()).map(|event| (i, event)));
                if let Some((i, event)) = candidate {
                    let candidate = candidates.remove(i);
                    *self = Self::Transform(Some(candidate));
                    Some(event)
                } else {
                    *self = Self::Transform(None);
                    warn!("No applicable transform.");
                    None
                }
            }
            Self::Transform(Some(transform)) => transform.transform(event),
            Self::Transform(None) => Some(event),
        }
    }
}
