mod description;
mod generate;
mod marker;

pub use self::description::{ComponentDescription, ExampleError};
pub use self::generate::GenerateConfig;
pub use self::marker::{
    ApiComponent, ComponentMarker, EnrichmentTableComponent, GlobalOptionComponent,
    ProviderComponent, SecretsComponent, SinkComponent, SourceComponent, TransformComponent,
};

// Create some type aliases for the component marker/description types, and collect (register,
// essentially) any submissions for each respective component marker.
pub type ApiDescription = ComponentDescription<ApiComponent>;
pub type SourceDescription = ComponentDescription<SourceComponent>;
pub type TransformDescription = ComponentDescription<TransformComponent>;
pub type SecretsDescription = ComponentDescription<SecretsComponent>;
pub type SinkDescription = ComponentDescription<SinkComponent>;
pub type EnrichmentTableDescription = ComponentDescription<EnrichmentTableComponent>;
pub type ProviderDescription = ComponentDescription<ProviderComponent>;
pub type GlobalOptionDescription = ComponentDescription<GlobalOptionComponent>;

inventory::collect!(ApiDescription);
inventory::collect!(SourceDescription);
inventory::collect!(TransformDescription);
inventory::collect!(SecretsDescription);
inventory::collect!(SinkDescription);
inventory::collect!(EnrichmentTableDescription);
inventory::collect!(ProviderDescription);
inventory::collect!(GlobalOptionDescription);
