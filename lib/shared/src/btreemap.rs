#[macro_export]
macro_rules! btreemap {
    () => (::std::collections::BTreeMap::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (::structures::ordmap!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::std::collections::BTreeMap::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_btreemap() {
        use std::collections::BTreeMap;

        assert_eq!(::structures::ordmap! {}, BTreeMap::<(), ()>::new());

        let mut map = BTreeMap::new();
        map.insert(1, "1");
        assert_eq!(::structures::ordmap! { 1 => "1" }, map);

        let mut map = BTreeMap::new();
        map.insert("1", "one");
        map.insert("2", "two");
        assert_eq!(::structures::ordmap! { "1" => "one", "2" => "two" }, map);
    }
}
