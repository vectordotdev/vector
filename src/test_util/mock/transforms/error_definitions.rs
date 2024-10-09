use async_trait::async_trait;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{DataType, Input, LogNamespace, TransformOutput},
    schema::Definition,
    transform::Transform,
};
use vrl::value::Kind;

use crate::config::{OutputId, TransformConfig, TransformContext};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("It all went horribly wrong"))]
    ItAllWentHorriblyWrong,
}

/// Configuration for the `test_error_definition` transform.
#[configurable_component(transform("test_error_definition", "Test (error definition)"))]
#[derive(Clone, Debug, Default)]
pub struct ErrorDefinitionTransformConfig {}

impl_generate_config_from_default!(ErrorDefinitionTransformConfig);

#[async_trait]
#[typetag::serde(name = "test_error_definition")]
impl TransformConfig for ErrorDefinitionTransformConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: vector_lib::vrl_cache::VrlCacheRegistry,
        definitions: &[(OutputId, Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::all_bits(),
            definitions
                .iter()
                .map(|(output, definition)| {
                    (
                        output.clone(),
                        // Return a definition of Kind::never implying that we can never return a value.
                        Definition::new_with_default_metadata(
                            Kind::never(),
                            definition.log_namespaces().clone(),
                        ),
                    )
                })
                .collect(),
        )]
    }

    async fn build(&self, _: &TransformContext) -> crate::Result<Transform> {
        // Even though the definitions returned were `Kind::never`, build needs to be
        // called in order to return the Error.
        Err(Error::ItAllWentHorriblyWrong.into())
    }
}
