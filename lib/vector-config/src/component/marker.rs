/// An API component.
pub struct ApiComponent;
/// An enrichment table component.
pub struct EnrichmentTableComponent;

// A global option component.
pub struct GlobalOptionComponent;

/// A provider component.
pub struct ProviderComponent;

/// A secrets component.
pub struct SecretsComponent;

/// A sink component.
pub struct SinkComponent;

/// A source component.
pub struct SourceComponent;

// A transform component.
pub struct TransformComponent;

// Marker trait representing a component.
pub trait ComponentMarker: sealed::Sealed {}

impl ComponentMarker for ApiComponent {}
impl ComponentMarker for EnrichmentTableComponent {}
impl ComponentMarker for GlobalOptionComponent {}
impl ComponentMarker for ProviderComponent {}
impl ComponentMarker for SecretsComponent {}
impl ComponentMarker for SinkComponent {}
impl ComponentMarker for SourceComponent {}
impl ComponentMarker for TransformComponent {}

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::ApiComponent {}
    impl Sealed for super::EnrichmentTableComponent {}
    impl Sealed for super::GlobalOptionComponent {}
    impl Sealed for super::ProviderComponent {}
    impl Sealed for super::SecretsComponent {}
    impl Sealed for super::SinkComponent {}
    impl Sealed for super::SourceComponent {}
    impl Sealed for super::TransformComponent {}
}
