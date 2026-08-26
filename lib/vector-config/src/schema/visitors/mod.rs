mod flattened_optional;
mod human_name;
mod inline_single;
pub mod merge;
pub mod scoped_visit;
mod unevaluated;

#[cfg(test)]
mod test;

pub use self::{
    flattened_optional::RewriteFlattenedOptionalVisitor,
    human_name::GenerateHumanFriendlyNameVisitor, inline_single::InlineSingleUseReferencesVisitor,
    unevaluated::DisallowUnevaluatedPropertiesVisitor,
};
