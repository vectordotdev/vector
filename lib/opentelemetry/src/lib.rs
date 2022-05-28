mod proto;

pub use proto::resource::v1::Resource;
pub use proto::common::v1 as Common;
pub use proto::logs::v1 as Logs;
pub use proto::collector::logs::v1 as LogService;

impl From<Logs::InstrumentationLibraryLogs> for Logs::ScopeLogs {
    fn from(v: Logs::InstrumentationLibraryLogs) -> Self {
        Self {
            scope: v.instrumentation_library.map(|v| Common::InstrumentationScope{
                name: v.name,
                version: v.version,
            }),
            log_records: v.log_records,
            schema_url: v.schema_url,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
