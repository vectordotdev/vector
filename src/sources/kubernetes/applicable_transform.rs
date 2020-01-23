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
                    // It could happen if user used:
                    //  - The newest Kubernetes which had breaking change related to logging,
                    //    and we haven't updated vector.
                    //    Currently v1.17
                    //
                    //  - One of older, pre CRI, Kubernetes version.
                    //    Theoretically, v1.5 is the lowest workable version.
                    //    Confirmed to work since v1.13
                    //
                    //  - Container runtime with alpha/beta/buggy implementation of CRI.
                    //
                    // CRI current version: https://github.com/kubernetes/kubernetes/tree/master/staging/src/k8s.io/cri-api/pkg/apis/runtime/v1alpha2
                    warn!("Unsupported Kubernetes version and Container runtime pair. Try changing one or the other, and consult with our documentation.");
                    None
                }
            }
            Self::Transform(Some(transform)) => transform.transform(event),
            Self::Transform(None) => Some(event),
        }
    }
}
