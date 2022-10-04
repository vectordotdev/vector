mod description;
mod generate;
mod marker;

pub use self::description::{ComponentDescription, ExampleError};
pub use self::generate::GenerateConfig;
pub use self::marker::{
    ComponentMarker, EnrichmentTableComponent, ProviderComponent, SecretsComponent, SinkComponent,
    SourceComponent, TransformComponent,
};

// Create some type aliases for the component marker/description types, and collect (register,
// essentially) any submissions for each respective component marker.
pub type SourceDescription = ComponentDescription<SourceComponent>;
pub type TransformDescription = ComponentDescription<TransformComponent>;
pub type SecretsDescription = ComponentDescription<SecretsComponent>;
pub type SinkDescription = ComponentDescription<SinkComponent>;
pub type EnrichmentTableDescription = ComponentDescription<EnrichmentTableComponent>;
pub type ProviderDescription = ComponentDescription<ProviderComponent>;

inventory::collect!(SourceDescription);
inventory::collect!(TransformDescription);
inventory::collect!(SecretsDescription);
inventory::collect!(SinkDescription);
inventory::collect!(EnrichmentTableDescription);
inventory::collect!(ProviderDescription);
