use super::{sink, source, transform, Component};
use lazy_static::lazy_static;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

pub const INVARIANT: &str = "Couldn't acquire lock on Vector components. Please report this.";

lazy_static! {
    pub static ref COMPONENTS: Arc<RwLock<HashMap<String, Component>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Filter components with the provided `map_func`
pub fn filter_components<T>(map_func: impl Fn((&String, &Component)) -> Option<T>) -> Vec<T> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .iter()
        .filter_map(map_func)
        .collect()
}

/// Returns all components
pub fn get_components() -> Vec<Component> {
    filter_components(|(_name, components)| Some(components.clone()))
}

/// Filters components, and returns a clone of sources
pub fn get_sources() -> Vec<source::Source> {
    filter_components(|(_, components)| match components {
        Component::Source(s) => Some(s.clone()),
        _ => None,
    })
}

/// Filters components, and returns a clone of transforms
pub fn get_transforms() -> Vec<transform::Transform> {
    filter_components(|(_, components)| match components {
        Component::Transform(t) => Some(t.clone()),
        _ => None,
    })
}

/// Filters components, and returns a clone of sinks
pub fn get_sinks() -> Vec<sink::Sink> {
    filter_components(|(_, components)| match components {
        Component::Sink(s) => Some(s.clone()),
        _ => None,
    })
}

/// Returns the current component names as a HashSet
pub fn get_component_names() -> HashSet<String> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .keys()
        .cloned()
        .collect::<HashSet<String>>()
}

/// Gets a component by name
pub fn component_by_name(name: &str) -> Option<Component> {
    Some(COMPONENTS.read().expect(INVARIANT).get(name)?.clone())
}

/// Overwrites component state with new components.
pub fn update(new_components: HashMap<String, Component>) {
    *COMPONENTS.write().expect(INVARIANT) = new_components
}
