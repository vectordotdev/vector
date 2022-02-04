/// A macro to easily create a Value type
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
        $crate::Value::Map(::std::collections::BTreeMap::default())
    });

    ({$($($k1:literal)? $($k2:ident)?: $v:tt),+ $(,)?}) => ({
        let map = vec![$((String::from($($k1)? $(stringify!($k2))?), $crate::value!($v))),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, $crate::Value>>();

        $crate::Value::Map(map)
    });

    (null) => ({
        $crate::Value::Null
    });

    ($k:expr) => ({
        $crate::Value::from($k)
    });
}
