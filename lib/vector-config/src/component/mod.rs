use linkme::distributed_slice;

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

pub trait Inventoried: Sized {
    fn iter() -> &'static [Self];
}

#[distributed_slice]
pub static SOURCES: [SourceDescription] = [..];

impl Inventoried for SourceDescription {
    fn iter() -> &'static [Self] {
        &SOURCES
    }
}

#[distributed_slice]
pub static TRANSFORMS: [TransformDescription] = [..];

impl Inventoried for TransformDescription {
    fn iter() -> &'static [Self] {
        &TRANSFORMS
    }
}

#[distributed_slice]
pub static SECRETS: [SecretsDescription] = [..];

impl Inventoried for SecretsDescription {
    fn iter() -> &'static [Self] {
        &SECRETS
    }
}

#[distributed_slice]
pub static SINKS: [SinkDescription] = [..];

impl Inventoried for SinkDescription {
    fn iter() -> &'static [Self] {
        &SINKS
    }
}

#[distributed_slice]
pub static ENRICHMENT_TABLES: [EnrichmentTableDescription] = [..];

impl Inventoried for EnrichmentTableDescription {
    fn iter() -> &'static [Self] {
        &ENRICHMENT_TABLES
    }
}

#[distributed_slice]
pub static PROVIDERS: [ProviderDescription] = [..];

impl Inventoried for ProviderDescription {
    fn iter() -> &'static [Self] {
        &PROVIDERS
    }
}
