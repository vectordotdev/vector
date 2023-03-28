use super::{visit::Visitor, Map, RootSchema, Schema, SchemaObject, DEFINITIONS_PREFIX};

/// Settings to customize how schemas are generated.
#[derive(Debug)]
pub struct SchemaSettings {
    /// A JSON pointer to the expected location of referenceable subschemas within the resulting root schema.
    definitions_path: String,

    /// The URI of the meta-schema describing the structure of the generated schemas.
    meta_schema: String,

    /// A list of visitors that get applied to all generated root schemas.
    visitors: Vec<Box<dyn Visitor>>,
}

impl Default for SchemaSettings {
    fn default() -> SchemaSettings {
        SchemaSettings::new()
    }
}

impl SchemaSettings {
    /// Creates `SchemaSettings` that conform to [JSON Schema 2019-09][json_schema_2019_09].
    ///
    /// [json_schema_2019_09]: https://json-schema.org/specification-links.html#2019-09-formerly-known-as-draft-8
    pub fn new() -> SchemaSettings {
        SchemaSettings {
            definitions_path: DEFINITIONS_PREFIX.to_owned(),
            meta_schema: "https://json-schema.org/draft/2019-09/schema".to_string(),
            visitors: Vec::default(),
        }
    }

    /// Gets the definitions path used by this generator.
    pub fn definitions_path(&self) -> &str {
        &self.definitions_path
    }

    /// Creates a `Visitor` from the given closure and appends it to the list of
    /// [visitors](SchemaSettings::visitors) for these `SchemaSettings`.
    #[allow(rustdoc::private_intra_doc_links)]
    pub fn with_visitor<F, V>(mut self, visitor_fn: F) -> Self
    where
        F: FnOnce(&Self) -> V,
        V: Visitor + 'static,
    {
        let visitor = visitor_fn(&self);
        self.visitors.push(Box::new(visitor));
        self
    }

    /// Creates a new [`SchemaGenerator`] using these settings.
    pub fn into_generator(self) -> SchemaGenerator {
        SchemaGenerator::new(self)
    }
}

/// Schema generator.
///
/// This is the main entrypoint for storing the defined schemas within a given root schema, and
/// referencing existing schema definitions.
#[derive(Debug, Default)]
pub struct SchemaGenerator {
    settings: SchemaSettings,
    definitions: Map<String, Schema>,
}

impl From<SchemaSettings> for SchemaGenerator {
    fn from(settings: SchemaSettings) -> Self {
        settings.into_generator()
    }
}

impl SchemaGenerator {
    /// Creates a new `SchemaGenerator` using the given settings.
    pub fn new(settings: SchemaSettings) -> SchemaGenerator {
        SchemaGenerator {
            settings,
            ..Default::default()
        }
    }

    /// Gets the [`SchemaSettings`] being used by this `SchemaGenerator`.
    pub fn settings(&self) -> &SchemaSettings {
        &self.settings
    }

    /// Borrows the collection of all [referenceable](JsonSchema::is_referenceable) schemas that
    /// have been generated.
    ///
    /// The keys of the returned `Map` are the [schema names](JsonSchema::schema_name), and the
    /// values are the schemas themselves.
    #[allow(rustdoc::broken_intra_doc_links)]
    pub fn definitions(&self) -> &Map<String, Schema> {
        &self.definitions
    }

    /// Mutably borrows the collection of all [referenceable](JsonSchema::is_referenceable) schemas
    /// that have been generated.
    ///
    /// The keys of the returned `Map` are the [schema names](JsonSchema::schema_name), and the
    /// values are the schemas themselves.
    #[allow(rustdoc::broken_intra_doc_links)]
    pub fn definitions_mut(&mut self) -> &mut Map<String, Schema> {
        &mut self.definitions
    }

    /// Attempts to find the schema that the given `schema` is referencing.
    ///
    /// If the given `schema` has a [`$ref`](../schema/struct.SchemaObject.html#structfield.reference)
    /// property which refers to another schema in `self`'s schema definitions, the referenced
    /// schema will be returned.  Otherwise, returns `None`.
    pub fn dereference<'a>(&'a self, schema: &Schema) -> Option<&'a Schema> {
        match schema {
            Schema::Object(SchemaObject {
                reference: Some(ref schema_ref),
                ..
            }) => {
                let definitions_path = &self.settings().definitions_path;
                if schema_ref.starts_with(definitions_path) {
                    let name = &schema_ref[definitions_path.len()..];
                    self.definitions.get(name)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Converts this generator into a root schema, using the given `root_schema` as the top-level
    /// definition.
    ///
    /// This assumes the root schema was generated using this generator, such that any schema
    /// definitions referenced by `root_schema` refer to this generator.
    ///
    /// All other relevant settings (i.e. meta-schema) are carried over.
    pub fn into_root_schema(mut self, root_schema: SchemaObject) -> RootSchema {
        let mut root_schema = RootSchema {
            meta_schema: Some(self.settings.meta_schema),
            schema: root_schema,
            definitions: self.definitions,
        };

        for visitor in self.settings.visitors.iter_mut() {
            visitor.visit_root_schema(&mut root_schema);
        }

        root_schema
    }
}
