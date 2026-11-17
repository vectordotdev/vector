use vector_config::configurable_component;

/// Materialization of named and numeric regex capture maps.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegexCaptureMap {
    /// Copy captured strings independently, retaining the existing ownership policy.
    #[default]
    Owned,
    /// Share one owned copy of each full match between its captured fields.
    /// Retains that full match while any captured field remains live.
    Shared,
}

impl RegexCaptureMap {
    pub(crate) const fn vrl(self) -> vrl::value::regex_capture::RegexCaptureMapMode {
        match self {
            Self::Owned => vrl::value::regex_capture::RegexCaptureMapMode::Owned,
            Self::Shared => vrl::value::regex_capture::RegexCaptureMapMode::Shared,
        }
    }
}
