use crate::{Function, Object, State};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Context {
    state: State,
    object: Box<dyn Object>,
    functions: HashMap<String, Box<dyn Function>>,
}
