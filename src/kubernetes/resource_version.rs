//! A resource version types to ensure proper usage protocol.
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, WatchEvent};
use k8s_openapi::Metadata;

/// Resource version state in the context of a chain of watch requests.
#[derive(Debug, Clone, Default)]
pub struct State(Option<String>);

impl State {
    /// Create a new resource version [`State`].
    pub fn new() -> Self {
        Self(None)
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
        T: Resource,
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
        object.resource_version()
    }

    /// Create a new [`resource_version::Candidate`] from a [`String`] without
    /// controlling the meaningness of the call.
    ///
    /// The [`resource_version::Candidate`] is a newtype solely designed,
    /// to lift the constraints on the resource version lifecycle to
    /// the type-system level.
    /// Use this function with caution, as this constructor permits unsoundess
    /// in the system.
    pub fn new_unchecked(val: String) -> Self {
        Self(val)
    }
}

/// An abstract entity holding and exposing a resource version.
pub trait Resource {
    /// Extracts and returns resource version.
    fn resource_version(&self) -> Option<Candidate>;
}

impl<T> Resource for T
where
    T: Metadata<Ty = ObjectMeta>,
{
    fn resource_version(&self) -> Option<Candidate> {
        let metadata = self.metadata();

        let new_resource_version = match metadata.resource_version {
            Some(ref val) => val,
            None => {
                warn!(message = "Got empty resource version at object metadata.");
                return None;
            }
        };

        Some(Candidate(new_resource_version.clone()))
    }
}
