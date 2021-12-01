use datadog_search_syntax::Field;

pub trait Fielder {
    type IntoIter: IntoIterator<Item = Field>;

    fn build_fields(&mut self, attr: impl AsRef<str>) -> Self::IntoIter;
}
