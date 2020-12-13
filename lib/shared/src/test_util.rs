#[macro_export]
macro_rules! map {
    () => (
        ::std::collections::BTreeMap::new()
    );
    ($($k:tt: $v:expr),+ $(,)?) => {
        vec![$(($k.into(), $v.into())),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, _>>()
    };
}
