/// A component with a well-known name.
///
/// Users can derive this trait automatically by using the
/// [`component_name`][vector_config::component_name] macro on their structs/enums.
pub trait NamedComponent {
    /// Name of the component.
    const NAME: &'static str;
}
