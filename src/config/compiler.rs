use super::{builder::ConfigBuilder, Config};

pub fn compile(raw: ConfigBuilder) -> Result<Config, Vec<String>> {
    let initial = Config {
        global: raw.global,
        sources: raw.sources,
        sinks: raw.sinks,
        transforms: raw.transforms,
        tests: raw.tests,
        expansions: Default::default(),
    };
    Ok(initial)
}
