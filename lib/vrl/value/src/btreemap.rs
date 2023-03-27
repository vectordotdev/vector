/// A macro to easily create a map containing `Value`
#[macro_export]
macro_rules! btreemap {
    () => (::std::collections::BTreeMap::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (btreemap!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        ::std::collections::BTreeMap::from([
            $(
                ($key.into(), $value.into()),
            )*
        ])
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_btreemap() {
        use std::collections::BTreeMap;

        assert_eq!(btreemap! {}, BTreeMap::<usize, usize>::new());

        let mut map = BTreeMap::new();
        map.insert(1, "1");
        assert_eq!(btreemap! { 1 => "1" }, map);

        let mut map = BTreeMap::new();
        map.insert("1", "one");
        map.insert("2", "two");
        assert_eq!(btreemap! { "1" => "one", "2" => "two" }, map);
    }
}
