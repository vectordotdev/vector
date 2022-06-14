pub use proto::collector::logs::v1 as LogService;
pub use proto::common::v1 as Common;
pub use proto::logs::v1 as Logs;
pub use proto::resource::v1::Resource;

mod convert;
mod proto;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
