//! A resource version types to ensure proper usage protocol.

use k8s_openapi::{
    apimachinery::pkg::apis::meta::v1::{ObjectMeta, WatchEvent},
    Metadata,
};

/// Resource version state in the context of a chain of watch requests.
#[derive(Debug, Clone, Default)]
pub struct State(Option<String>);

impl State {
    /// Create a new resource version [`State`].
    pub fn new() -> Self {
        Self(Some("0".to_owned()))
    }

    /// Update the resource version from a candidate obtained earlier.
    ///
    /// Returns the previous state.
    pub fn update(&mut self, candidate: Candidate) -> Option<String> {
        self.0.replace(candidate.0)
    }

    /// Reset the resource version. Use in case of a desync.
    ///
    /// Returns the previous state.
    pub fn reset(&mut self) -> Option<String> {
        self.0.take()
    }

    /// Get the current resource version value.
    pub fn get(&self) -> Option<&str> {
        Some(self.0.as_ref()?.as_str())
    }
}

/// A resource version candidate, can be used to update the resource version.
pub struct Candidate(String);

impl Candidate {
    /// Obtain a resource version [`Candidate`] from a [`WatchEvent`].
    pub fn from_watch_event<T>(event: &WatchEvent<T>) -> Option<Self>
    where
        T: Metadata<Ty = ObjectMeta>,
    {
        let object = match event {
            WatchEvent::Added(object)
            | WatchEvent::Modified(object)
            | WatchEvent::Deleted(object) => object,
            WatchEvent::Bookmark { resource_version } => {
                return Some(Self(resource_version.clone()))
            }
            WatchEvent::ErrorStatus(_) | WatchEvent::ErrorOther(_) => return None,
        };
        Self::from_object(object)
    }

    /// Obtain a resource version [`Candidate`] from a object of type `T`.
    pub fn from_object<T>(object: &T) -> Option<Self>
    where
        T: Metadata<Ty = ObjectMeta>,
    {
        let metadata = object.metadata();

        let new_resource_version = match metadata.resource_version {
            Some(ref val) => val,
            None => {
                warn!(message = "Got empty resource version at object metadata.");
                return None;
            }
        };

        Some(Self(new_resource_version.clone()))
    }
}
