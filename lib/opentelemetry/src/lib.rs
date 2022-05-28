pub use proto::resource::v1::Resource;
pub use proto::common::v1 as Common;
pub use proto::logs::v1 as Logs;
pub use proto::collector::logs::v1 as LogService;

mod proto;
mod convert;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
