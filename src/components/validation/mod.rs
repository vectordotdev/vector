mod resources;
#[cfg(feature = "component-validation-runner")]
mod runner;
mod sync;
mod test_case;
pub mod util;
mod validators;

use vector_lib::config::LogNamespace;

use crate::config::{BoxedSink, BoxedSource, BoxedTransform};

/// For components implementing `ValidatableComponent`
pub mod prelude {
    pub use super::ComponentTestCaseConfig;
    pub use super::ExternalResource;
    pub use super::HttpResourceConfig;
    pub use super::ResourceDirection;
    pub use super::ValidatableComponent;
    pub use super::ValidationConfiguration;
    pub use crate::register_validatable_component;
}

pub use self::resources::*;
#[cfg(feature = "component-validation-runner")]
pub use self::runner::*;
pub use self::test_case::{TestCase, TestCaseExpectation};
pub use self::validators::*;

pub mod component_names {
    pub const TEST_SOURCE_NAME: &str = "test_source";
    pub const TEST_SINK_NAME: &str = "test_sink";
    pub const TEST_TRANSFORM_NAME: &str = "test_transform";
    pub const TEST_INPUT_SOURCE_NAME: &str = "input_source";
    pub const TEST_OUTPUT_SINK_NAME: &str = "output_sink";
}

/// Component types that can be validated.
// TODO: We should centralize this in `vector-common` or something, where both this code and the
// configuration schema stuff (namely the proc macros that use this) can share it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentType {
    Source,
    Transform,
    Sink,
}

impl ComponentType {
    /// Gets the name of this component type as a string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Transform => "transform",
            Self::Sink => "sink",
        }
    }
}

/// Component type-specific configuration.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum ComponentConfiguration {
    /// A source component.
    Source(BoxedSource),

    /// A transform component.
    Transform(BoxedTransform),

    /// A sink component.
    Sink(BoxedSink),
}

/// Component configuration for a test case.
#[derive(Clone)]
pub struct ComponentTestCaseConfig {
    config: ComponentConfiguration,
    /// If specified, this name must match the `config_name` field of at least one of the test case events.
    test_case: Option<String>,
    external_resource: Option<ExternalResource>,
}

impl ComponentTestCaseConfig {
    pub fn from_source<C: Into<BoxedSource>>(
        config: C,
        test_case: Option<String>,
        external_resource: Option<ExternalResource>,
    ) -> Self {
        Self {
            config: ComponentConfiguration::Source(config.into()),
            test_case,
            external_resource,
        }
    }
    pub fn from_transform<C: Into<BoxedTransform>>(
        config: C,
        test_case: Option<String>,
        external_resource: Option<ExternalResource>,
    ) -> Self {
        Self {
            config: ComponentConfiguration::Transform(config.into()),
            test_case,
            external_resource,
        }
    }
    pub fn from_sink<C: Into<BoxedSink>>(
        config: C,
        test_case: Option<String>,
        external_resource: Option<ExternalResource>,
    ) -> Self {
        Self {
            config: ComponentConfiguration::Sink(config.into()),
            test_case,
            external_resource,
        }
    }
}

/// Configuration for validating a component.
///
/// This type encompasses all of the required information for configuring and validating a
/// component, including the strongly-typed configuration for building a topology, as well as the
/// definition of the external resource required to properly interact with the component.
#[derive(Clone)]
pub struct ValidationConfiguration {
    component_name: &'static str,
    component_type: ComponentType,
    /// There may be only one `ComponentTestCaseConfig` necessary to execute all test cases, but some cases
    /// require more advanced configuration in order to hit the code path desired.
    component_configurations: Vec<ComponentTestCaseConfig>,
    log_namespace: LogNamespace,
}

impl ValidationConfiguration {
    /// Creates a new `ValidationConfiguration` for a source.
    pub fn from_source(
        component_name: &'static str,
        log_namespace: LogNamespace,
        component_configurations: Vec<ComponentTestCaseConfig>,
    ) -> Self {
        Self {
            component_name,
            component_type: ComponentType::Source,
            component_configurations,
            log_namespace,
        }
    }

    /// Creates a new `ValidationConfiguration` for a transform.
    pub fn from_transform(
        component_name: &'static str,
        log_namespace: LogNamespace,
        component_configurations: Vec<ComponentTestCaseConfig>,
    ) -> Self {
        Self {
            component_name,
            component_type: ComponentType::Transform,
            component_configurations,
            log_namespace,
        }
    }

    /// Creates a new `ValidationConfiguration` for a sink.
    pub fn from_sink(
        component_name: &'static str,
        log_namespace: LogNamespace,
        component_configurations: Vec<ComponentTestCaseConfig>,
    ) -> Self {
        Self {
            component_name,
            component_type: ComponentType::Sink,
            component_configurations,
            log_namespace,
        }
    }

    /// Gets the name of the component.
    pub const fn component_name(&self) -> &'static str {
        self.component_name
    }

    /// Gets the type of the component.
    pub const fn component_type(&self) -> ComponentType {
        self.component_type
    }

    /// Gets the configuration of the component.
    pub fn component_configurations(&self) -> Vec<ComponentTestCaseConfig> {
        self.component_configurations.clone()
    }

    /// Gets the LogNamespace that the component is using.
    pub const fn log_namespace(&self) -> LogNamespace {
        self.log_namespace
    }

    fn get_comp_test_case(&self, test_case: Option<&String>) -> Option<ComponentTestCaseConfig> {
        let empty = String::from("");
        let test_case = test_case.unwrap_or(&empty);
        self.component_configurations
            .clone()
            .into_iter()
            .find(|c| c.test_case.as_ref().unwrap_or(&String::from("")) == test_case)
    }

    /// Gets the configuration of the component.
    pub fn component_configuration_for_test_case(
        &self,
        test_case: Option<&String>,
    ) -> Option<ComponentConfiguration> {
        self.get_comp_test_case(test_case).map(|c| c.config)
    }

    /// Gets the external resource definition for validating the component, if any.
    pub fn external_resource(&self, test_case: Option<&String>) -> Option<ExternalResource> {
        self.get_comp_test_case(test_case)
            .and_then(|c| c.external_resource)
    }
}

pub trait ValidatableComponent: Send + Sync {
    /// Gets the validation configuration for this component.
    ///
    /// The validation configuration compromises the two main requirements for validating a
    /// component: how to configure the component in a topology, and what external resources, if
    /// any, it depends on.
    fn validation_configuration() -> ValidationConfiguration;
}

/// Description of a validatable component.
pub struct ValidatableComponentDescription {
    validation_configuration: fn() -> ValidationConfiguration,
}

impl ValidatableComponentDescription {
    /// Creates a new `ValidatableComponentDescription`.
    ///
    /// This creates a validatable component description for a component identified by the given
    /// component type `V`.
    pub const fn new<V: ValidatableComponent>() -> Self {
        Self {
            validation_configuration: <V as ValidatableComponent>::validation_configuration,
        }
    }

    /// Queries the list of validatable components for a component with the given name and component type.
    pub fn query(
        component_name: &str,
        component_type: ComponentType,
    ) -> Option<ValidationConfiguration> {
        inventory::iter::<Self>
            .into_iter()
            .map(|v| (v.validation_configuration)())
            .find(|v| v.component_name() == component_name && v.component_type() == component_type)
    }
}

inventory::collect!(ValidatableComponentDescription);

#[macro_export]
macro_rules! register_validatable_component {
    ($ty:ty) => {
        ::inventory::submit! {
            $crate::components::validation::ValidatableComponentDescription::new::<$ty>()
        }
    };
}

/// Input and Output runners populate this structure as they send and receive events.
/// The structure is passed into the validator to use as the expected values for the
/// metrics that the components under test actually output.
#[derive(Default, Debug)]
pub struct RunnerMetrics {
    pub received_events_total: u64,
    pub received_event_bytes_total: u64,
    pub received_bytes_total: u64,
    pub sent_bytes_total: u64,
    pub sent_event_bytes_total: u64,
    pub sent_events_total: u64,
    pub errors_total: u64,
    pub discarded_events_total: u64,
}

#[cfg(feature = "component-validation-runner")]
fn run_validation(configuration: ValidationConfiguration, test_case_data_path: std::path::PathBuf) {
    let component_name = configuration.component_name();
    info!(
        "Running validation for component '{}' (type: {:?})...",
        component_name,
        configuration.component_type()
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let mut runner = Runner::from_configuration(
            configuration,
            test_case_data_path,
            crate::extra_context::ExtraContext::default(),
        );
        runner.add_validator(StandardValidators::ComponentSpec);

        match runner.run_validation().await {
            Ok(test_case_results) => {
                let mut details = Vec::new();
                let mut had_failures = false;

                for test_case_result in test_case_results.into_iter() {
                    for validator_result in test_case_result.validator_results() {
                        match validator_result {
                            Ok(success) => {
                                if success.is_empty() {
                                    details.push(format!(
                                        "  test case '{}': passed",
                                        test_case_result.test_name()
                                    ));
                                } else {
                                    let formatted = success
                                        .iter()
                                        .map(|s| format!("    - {}\n", s))
                                        .collect::<Vec<_>>();

                                    details.push(format!(
                                        "  test case '{}': passed\n{}",
                                        test_case_result.test_name(),
                                        formatted.join("")
                                    ));
                                }
                            }
                            Err(failure) => {
                                had_failures = true;

                                if failure.is_empty() {
                                    details.push(format!(
                                        "  test case '{}': failed",
                                        test_case_result.test_name()
                                    ));
                                } else {
                                    let formatted = failure
                                        .iter()
                                        .map(|s| format!("    - {}\n", s))
                                        .collect::<Vec<_>>();

                                    details.push(format!(
                                        "  test case '{}': failed\n{}",
                                        test_case_result.test_name(),
                                        formatted.join("")
                                    ));
                                }
                            }
                        }
                    }
                }

                if had_failures {
                    panic!(
                        "Failed to validate component '{}':\n{}",
                        component_name,
                        details.join("")
                    );
                } else {
                    info!(
                        "Successfully validated component '{}':\n{}",
                        component_name,
                        details.join("")
                    );
                }
            }
            Err(e) => panic!(
                "Failed to complete validation run for component '{}': {}",
                component_name, e
            ),
        }
    });
}

#[cfg(feature = "component-validation-runner")]
fn get_validation_configuration_from_test_case_path(
    test_case_data_path: &std::path::Path,
) -> Result<ValidationConfiguration, String> {
    // The test case data path should follow a fixed structure where the 2nd to last segment is
    // the component type, and the last segment -- when the extension is removed -- is the
    // component name.
    let mut path_segments = test_case_data_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(path) => Some(std::path::Path::new(path)),
            _ => None,
        })
        .collect::<std::collections::VecDeque<_>>();
    if path_segments.len() <= 2 {
        return Err(format!(
            "Test case data path contained {} normal path segment(s), expected at least 2 or more.",
            path_segments.len()
        ));
    }

    let component_name = path_segments
        .pop_back()
        .and_then(|segment| segment.file_stem().map(|s| s.to_string_lossy().to_string()))
        .ok_or(format!(
            "Test case data path '{}' contained unexpected or invalid filename.",
            test_case_data_path.as_os_str().to_string_lossy()
        ))?;

    let component_type = path_segments
        .pop_back()
        .map(|segment| {
            segment
                .as_os_str()
                .to_string_lossy()
                .to_string()
                .to_ascii_lowercase()
        })
        .and_then(|segment| match segment.as_str() {
            "sources" => Some(ComponentType::Source),
            "transforms" => Some(ComponentType::Transform),
            "sinks" => Some(ComponentType::Sink),
            _ => None,
        })
        .ok_or(format!(
            "Test case data path '{}' contained unexpected or invalid component type.",
            test_case_data_path.as_os_str().to_string_lossy()
        ))?;

    // Now that we've theoretically got the component type and component name, try to query the
    // validatable component descriptions to find it.
    ValidatableComponentDescription::query(&component_name, component_type).ok_or(format!(
        "No validation configuration for component '{}' with component type '{}'.",
        component_name,
        component_type.as_str()
    ))
}

#[cfg(feature = "component-validation-runner")]
pub fn validate_component(test_case_data_path: std::path::PathBuf) {
    if !test_case_data_path.exists() {
        panic!("Component validation test invoked with path to test case data that could not be found: {}", test_case_data_path.to_string_lossy());
    }

    let configuration = get_validation_configuration_from_test_case_path(&test_case_data_path)
        .expect("Failed to find validation configuration from given test case data path.");

    run_validation(configuration, test_case_data_path);
}

#[cfg(all(test, feature = "component-validation-tests"))]
mod tests {
    #[test_generator::test_resources("tests/validation/components/**/*.yaml")]
    pub fn validate_component(test_case_data_path: &str) {
        crate::test_util::trace_init();

        let test_case_data_path = std::path::PathBuf::from(test_case_data_path.to_string());

        super::validate_component(test_case_data_path);
    }
}
