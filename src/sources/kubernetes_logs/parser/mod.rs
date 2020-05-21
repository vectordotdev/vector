use crate::transforms::{
    util::pick::{IterPicker, Passthrough, PickOnce},
    Transform,
};

mod cri;
mod docker;
mod test_util;

/// Parser for any log format supported by `kubelet`.
pub type Parser = PickOnce<IterPicker<Vec<Box<dyn Transform>>>>;

/// Build a parser for any log format supported by `kubelet`.
pub fn build() -> Parser {
    let pickers = vec![docker::build(), cri::build(), Box::new(Passthrough)];
    PickOnce::new(IterPicker::new(pickers))
}
