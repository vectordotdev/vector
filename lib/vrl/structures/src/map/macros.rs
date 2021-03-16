#[macro_export]
macro_rules! map {
    () => (::vrl_structures::map::Map::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (map!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::vrl_structures::map::Map::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}
