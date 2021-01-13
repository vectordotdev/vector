use async_graphql::InputObject;
use smallvec::{smallvec, SmallVec};

/// Takes an &Option<bool> and returns early if false
#[macro_export]
macro_rules! filter_check {
    ($($match:expr),+) => {
        $(
            if matches!($match, Some(t) if !t) {
                return false;
            }
        )+
    }
}

#[derive(Default, InputObject)]
/// Filter for String values
pub struct StringFilter {
    equals: Option<String>,
    not_equals: Option<String>,
    contains: Option<String>,
    not_contains: Option<String>,
    starts_with: Option<String>,
    ends_with: Option<String>,
}

impl StringFilter {
    pub fn filter_value(&self, value: &str) -> bool {
        filter_check!(
            // Equals
            self.equals.as_ref().map(|s| value.eq(s)),
            // Not equals
            self.not_equals.as_ref().map(|s| !value.eq(s)),
            // Contains
            self.contains.as_ref().map(|s| value.contains(s)),
            // Does not contain
            self.not_contains.as_ref().map(|s| !value.contains(s)),
            // Starts with
            self.starts_with.as_ref().map(|s| value.starts_with(s)),
            // Ends with
            self.ends_with.as_ref().map(|s| value.ends_with(s))
        );
        true
    }
}

pub trait CustomFilter<T> {
    fn matches(&self, item: &T) -> bool;
    fn or(&self) -> Option<&Vec<Self>>
    where
        Self: Sized;
}

pub fn filter_items<Item, Iter, Filter>(items: Iter, f: &Filter) -> Vec<Item>
where
    Item: Clone,
    Iter: Iterator<Item = Item>,
    Filter: CustomFilter<Item>,
{
    items
        .filter(|c| {
            f.matches(c)
                || f.or().map_or_else(
                    || false,
                    |f| {
                        f.into_iter().any(|f| {
                            let items: SmallVec<[Item; 1]> = smallvec![(*c).clone()];
                            filter_items(items.into_iter(), f).len() > 0
                        })
                    },
                )
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::StringFilter;

    #[test]
    fn string_equals() {
        let value = "test";

        let mut sf = StringFilter::default();
        sf.equals = value.to_string().into();

        assert!(sf.filter_value(&value));
        assert!(!sf.filter_value("not found"));
    }

    #[test]
    fn string_not_equals() {
        let value = "value";
        let diff_value = "different value";

        let mut sf = StringFilter::default();
        sf.not_equals = diff_value.to_string().into();

        assert!(sf.filter_value(&value));
        assert!(!sf.filter_value(diff_value));
    }

    #[test]
    fn string_contains() {
        let mut sf = StringFilter::default();
        sf.contains = "234".to_string().into();

        assert!(sf.filter_value("12345"));
        assert!(!sf.filter_value("xxx"));
    }

    #[test]
    fn string_not_contains() {
        let contains = "xyz";
        let mut sf = StringFilter::default();
        sf.not_contains = contains.to_string().into();

        assert!(sf.filter_value("abc"));
        assert!(!sf.filter_value(contains));
    }

    #[test]
    fn string_starts_with() {
        let mut sf = StringFilter::default();
        sf.starts_with = "abc".to_string().into();

        assert!(sf.filter_value("abcdef"));
        assert!(!sf.filter_value("xyz"));
    }

    #[test]
    fn string_ends_with() {
        let mut sf = StringFilter::default();
        sf.ends_with = "456".to_string().into();

        assert!(sf.filter_value("123456"));
        assert!(!sf.filter_value("123"));
    }

    #[test]
    fn string_multiple_all_match() {
        let value = "123456";
        let mut sf = StringFilter::default();

        sf.equals = value.to_string().into();
        sf.not_equals = "xyz".to_string().into();
        sf.contains = "234".to_string().into();
        sf.not_contains = "678".to_string().into();
        sf.starts_with = "123".to_string().into();
        sf.ends_with = "456".to_string().into();

        assert!(sf.filter_value(value));
        assert!(!sf.filter_value("should fail"));
    }
}
