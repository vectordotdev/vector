mod renderer;

use std::collections::HashMap;

use crate::renderer::SchemaRenderer;
use anyhow::{Context, Result};
use tracing::debug;
use vector_config::schema::parser::{component::ComponentSchema, query::SchemaQuerier};
use vector_config_common::constants::{self, ComponentType};

fn main() -> Result<()> {
    let querier = SchemaQuerier::from_schema("/tmp/vector-config-schema.json")
        .context("Failed to create querier from given schema file path.")?;

    let base_component_types = &[
        ComponentType::Source,
        ComponentType::Transform,
        ComponentType::Sink,
    ];
    for base_component_type in base_component_types {
        // Find the base component schema for the component type itself, which is analogous to
        // `SourceOuter`, `SinkOuter`, etc. We render the schema for that separately as it's meant
        // to be common across components of the same type, etc.
        let base_component_schema = querier
            .query()
            .with_custom_attribute_kv(
                constants::DOCS_META_COMPONENT_BASE_TYPE,
                base_component_type,
            )
            .run_single()?;

        debug!(
            "Got base component schema for component type '{}'.",
            base_component_type.as_str()
        );

        // Find all component schemas of the same component type.
        let maybe_component_schemas = querier
            .query()
            .with_custom_attribute_kv(constants::DOCS_META_COMPONENT_TYPE, base_component_type)
            .run()
            .into_iter()
            .map(ComponentSchema::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        debug!(
            "Found {} component schema(s) for component type '{}'.",
            maybe_component_schemas.len(),
            base_component_type.as_str()
        );

        let mut rendered_component_schemas = HashMap::new();

        // Render the base component schema.
        let base_component_schema_renderer = SchemaRenderer::new(&querier, base_component_schema);
        let rendered_base_component_schema =
            base_component_schema_renderer.render().context(format!(
                "Failed to render the base component schema for component type '{}'.",
                base_component_type.as_str()
            ))?;
        rendered_component_schemas.insert(
            format!("base/{}", base_component_type.as_str()),
            rendered_base_component_schema,
        );

        // Render each of the component schemas for this component type.
        for component_schema in maybe_component_schemas {
            let component_name = component_schema.component_name().to_string();
            let component_schema_renderer = SchemaRenderer::new(&querier, component_schema);
            let rendered_component_schema = component_schema_renderer.render().context(format!(
                "Failed to render the '{}' component schema.",
                component_name
            ))?;
            rendered_component_schemas.insert(
                format!("{}s/base/{}", base_component_type.as_str(), component_name),
                rendered_component_schema,
            );
        }
    }

    Ok(())
}
