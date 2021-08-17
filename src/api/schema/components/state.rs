use super::{sink, source, transform, Component};
use crate::config::ComponentId;
use lazy_static::lazy_static;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

pub const INVARIANT: &str = "Couldn't acquire lock on Vector components. Please report this.";

lazy_static! {
    pub static ref COMPONENTS: Arc<RwLock<HashMap<ComponentId, Component>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Filter components with the provided `map_func`
pub fn filter_components<T>(map_func: impl Fn((&ComponentId, &Component)) -> Option<T>) -> Vec<T> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .iter()
        .filter_map(map_func)
        .collect()
}

/// Returns all components
pub fn get_components() -> Vec<Component> {
    filter_components(|(_component_id, components)| Some(components.clone()))
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

/// Returns the current component component_ids as a HashSet
pub fn get_component_ids() -> HashSet<ComponentId> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .keys()
        .cloned()
        .collect::<HashSet<ComponentId>>()
}

/// Gets a component by component_id
pub fn component_by_component_id(component_id: &ComponentId) -> Option<Component> {
    Some(
        COMPONENTS
            .read()
            .expect(INVARIANT)
            .get(component_id)?
            .clone(),
    )
}

/// Overwrites component state with new components.
pub fn update(new_components: HashMap<ComponentId, Component>) {
    *COMPONENTS.write().expect(INVARIANT) = new_components
}
