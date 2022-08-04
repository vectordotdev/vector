#[macro_export]
macro_rules! value {
    ([]) => ({
        ::value::Value::Array(vec![])
    });

    ([$($v:tt),+ $(,)?]) => ({
        let vec: Vec<::value::Value> = vec![$($crate::value!($v)),+];
        ::value::Value::Array(vec)
    });

    ({}) => ({
        ::value::Value::Object(::std::collections::BTreeMap::default())
    });

    ({$($($k1:literal)? $($k2:ident)?: $v:tt),+ $(,)?}) => ({
        let map = vec![$((String::from($($k1)? $(stringify!($k2))?), $crate::value!($v))),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, ::value::Value>>();

        ::value::Value::Object(map)
    });

    (null) => ({
        ::value::Value::Null
    });

    ($k:expr) => ({
        ::value::Value::from($k)
    });
}
