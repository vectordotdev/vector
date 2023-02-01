use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use once_cell::sync::Lazy;

use super::{sink, source, transform, Component};
use crate::config::{ComponentKey, OutputId};

pub const INVARIANT: &str = "Couldn't acquire lock on Vector components. Please report this.";

pub static COMPONENTS: Lazy<Arc<RwLock<HashMap<ComponentKey, Component>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Filter components with the provided `map_func`
pub fn filter_components<T>(map_func: impl Fn((&ComponentKey, &Component)) -> Option<T>) -> Vec<T> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .iter()
        .filter_map(map_func)
        .collect()
}

/// Returns all components
pub fn get_components() -> Vec<Component> {
    filter_components(|(_component_key, components)| Some(components.clone()))
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

/// Returns the current component component_keys as a HashSet
pub fn get_component_keys() -> HashSet<ComponentKey> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .keys()
        .cloned()
        .collect::<HashSet<ComponentKey>>()
}

/// Gets a component by component_key
pub fn component_by_component_key(component_key: &ComponentKey) -> Option<Component> {
    Some(
        COMPONENTS
            .read()
            .expect(INVARIANT)
            .get(component_key)?
            .clone(),
    )
}

/// Gets a component by output_id
pub fn component_by_output_id(output_id: &OutputId) -> Option<Component> {
    filter_components(|(key, component)| {
        if key == &output_id.component {
            Some(component.clone())
        } else {
            None
        }
    })
    .pop()
}

/// Overwrites component state with new components.
pub fn update(new_components: HashMap<ComponentKey, Component>) {
    *COMPONENTS.write().expect(INVARIANT) = new_components
}
