#[macro_export]
macro_rules! value {
    ([]) => ({
        $crate::Value::Array(vec![])
    });

    ([$($v:tt),+ $(,)?]) => ({
        let vec: Vec<$crate::Value> = vec![$($crate::value!($v)),+];
        $crate::Value::Array(vec)
    });

    ({}) => ({
        $crate::Value::Object(::structures::map::ord::Map::default())
    });

    ({$($($k1:literal)? $($k2:ident)?: $v:tt),+ $(,)?}) => ({
        let map = vec![$((String::from($($k1)? $(stringify!($k2))?), $crate::value!($v))),+]
            .into_iter()
            .collect::<::structures::map::ord::Map<_, $crate::Value>>();

        $crate::Value::Object(map)
    });

    (null) => ({
        $crate::Value::Null
    });

    ($k:expr) => ({
        $crate::Value::from($k)
    });
}
