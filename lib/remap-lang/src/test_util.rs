#[macro_export]
macro_rules! test_type_def {
    ($($name:ident { expr: $expr:expr, def: $def:expr, })+) => {
        mod type_def {
            use super::*;

            $(
                #[test]
                fn $name() {
                    let mut state = state::Compiler::default();
                    let expression: Box<dyn Expression> = Box::new($expr(&mut state));

                    assert_eq!(expression.type_def(&state), $def);
                }
            )+
        }
    };
}

#[macro_export]
macro_rules! func_args {
    () => (
        ::std::collections::HashMap::<&'static str, $crate::Expr>::default()
    );
    ($($k:tt: $v:expr),+ $(,)?) => {
        vec![$((stringify!($k), $v.into())),+]
            .into_iter()
            .collect::<::std::collections::HashMap<&'static str, $crate::Expr>>()
    };
}

#[macro_export]
macro_rules! bench_function {
    ($name:tt => $func:path; $($case:ident { args: $args:expr, want: $(Ok($ok:expr))? $(Err($err:expr))? $(,)* })+) => {
        fn $name(c: &mut criterion::Criterion) {
            $(
                c.bench_function(&format!("{}: {}", stringify!($name), stringify!($case)), |b| {
                    let (expression, want) = $crate::__prep_bench_or_test!($func, $args, $(Ok($crate::Value::from($ok)))? $(Err($err.to_owned()))?);
                    let mut state = $crate::state::Program::default();
                    let mut object: $crate::Value = ::std::collections::BTreeMap::default().into();

                    b.iter(|| {
                        let got = expression.execute(&mut state, &mut object).map_err(|e| e.to_string());
                        debug_assert_eq!(got, want);
                        got
                    })
                });
            )+
        }
    };
}

#[macro_export]
macro_rules! test_function {
    ($name:tt => $func:path; $($case:ident { args: $args:expr, want: $(Ok($ok:expr))? $(Err($err:expr))? $(,)* })+) => {
        $crate::paste!{$(
        #[test]
        fn [<$name _ $case:snake:lower>]() {
            let (expression, want) = $crate::__prep_bench_or_test!($func, $args, $(Ok($crate::Value::from($ok)))? $(Err($err.to_owned()))?);
            let mut state = $crate::state::Program::default();
            let mut object: $crate::Value = ::std::collections::BTreeMap::default().into();

            let got = expression.execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, want);
        }
        )+}
    };
}

#[macro_export]
macro_rules! map {
    () => ({
        let map = ::std::collections::BTreeMap::<String, $crate::Expr>::new();
        $crate::expression::Map::new(map)
    });
    ($($k:tt: $v:expr),+ $(,)?) => ({
        let map: ::std::collections::BTreeMap<String, $crate::Expr> = vec![$(($k.into(), $v.into())),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, _>>();

        $crate::expression::Map::new(map)
    });
}

#[macro_export]
macro_rules! array {
    () => ({
        let vec: Vec<$crate::Value> = ::std::vec::Vec::new();
        $crate::expression::Array::from(vec)
    });
    ($($v:expr),+ $(,)?) => ({
        let vec: Vec<$crate::Value> = vec![$($v.into()),+];
        $crate::expression::Array::from(vec)
    })
}

/// Create a `Literal` expression type.
#[macro_export]
macro_rules! lit {
    ($v:tt) => {
        $crate::expression::Literal::from($crate::value!($v))
    };
}

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

#[doc(hidden)]
#[macro_export]
macro_rules! __prep_bench_or_test {
    ($func:path, $args:expr, $want:expr) => {{
        let args: ::std::collections::HashMap<&str, $crate::Expr> = $args;

        let mut arguments = $crate::function::ArgumentList::default();
        for (k, v) in args {
            arguments.insert(k, v)
        }

        ($func.compile(arguments).unwrap(), $want)
    }};
}
