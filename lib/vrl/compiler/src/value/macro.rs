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
        $crate::Value::Object(::std::collections::BTreeMap::default())
    });

    ({$($($k1:literal)? $($k2:ident)?: $v:tt),+ $(,)?}) => ({
        let map = vec![$((String::from($($k1)? $(stringify!($k2))?), $crate::value!($v))),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, $crate::SharedValue>>();

        $crate::Value::Object(map)
    });

    (null) => ({
        $crate::Value::Null
    });

    ($k:expr) => ({
        $crate::Value::from($k)
    });
}

#[macro_export]
macro_rules! shared_value {
    ([]) => ({
        $crate::SharedValue::from($crate::Value::Array(vec![]))
    });

    ([$($v:tt),+ $(,)?]) => ({
        let vec: Vec<$crate::Value> = vec![$($crate::value!($v)),+];
        $crate::SharedValue::from($crate::Value::Array(vec))
    });

    ({}) => ({
        $crate::SharedValue::from($crate::Value::Object(::std::collections::BTreeMap::default()))
    });

    ({$($($k1:literal)? $($k2:ident)?: $v:tt),+ $(,)?}) => ({
        let map = vec![$((String::from($($k1)? $(stringify!($k2))?), $crate::value!($v))),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, $crate::SharedValue>>();

        $crate::SharedValue::from($crate::Value::Object(map))
    });

    (null) => ({
        $crate::SharedValue::from($crate::Value::Null)
    });

    ($k:expr) => ({
        $crate::SharedValue::from($crate::Value::from($k))
    });
}
