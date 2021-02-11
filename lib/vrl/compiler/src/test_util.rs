/// Create a boxed [`Expression`] trait object from a given [`Value`].
///
/// Supports the same format as the [`value`] macro.
#[macro_export]
macro_rules! expr {
    ($($v:tt)*) => {{
        let value = $crate::value!($($v)*);
        value.into_expression()
    }};
}

#[macro_export]
macro_rules! test_type_def {
    ($($name:ident { expr: $expr:expr, want: $def:expr, })+) => {
        mod type_def {
            use super::*;

            $(
                #[test]
                fn $name() {
                    let mut state = $crate::state::Compiler::default();
                    let expression = Box::new($expr(&mut state));

                    assert_eq!(expression.type_def(&state), $def);
                }
            )+
        }
    };
}

#[macro_export]
macro_rules! func_args {
    () => (
        ::std::collections::HashMap::<&'static str, $crate::Value>::default()
    );
    ($($k:tt: $v:expr),+ $(,)?) => {
        vec![$((stringify!($k), $v.into())),+]
            .into_iter()
            .collect::<::std::collections::HashMap<&'static str, $crate::Value>>()
    };
}

#[macro_export]
macro_rules! bench_function {
    ($name:tt => $func:path; $($case:ident { args: $args:expr, want: $(Ok($ok:expr))? $(Err($err:expr))? $(,)* })+) => {
        fn $name(c: &mut criterion::Criterion) {
            $(
                c.bench_function(&format!("{}: {}", stringify!($name), stringify!($case)), |b| {
                    let (expression, want) = $crate::__prep_bench_or_test!($func, $args, $(Ok($crate::Value::from($ok)))? $(Err($err.to_owned()))?);
                    let mut compiler_state = $crate::state::Compiler::default();
                    let mut runtime_state = $crate::state::Runtime::default();
                    let mut target: $crate::Value = ::std::collections::BTreeMap::default().into();
                    let mut ctx = $crate::Context::new(&mut target, &mut runtime_state);

                    b.iter(|| {
                        let got = expression.resolve(&mut ctx).map_err(|e| e.to_string());
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
    ($name:tt => $func:path; $($case:ident { args: $args:expr, want: $(Ok($ok:expr))? $(Err($err:expr))?, tdef: $tdef:expr,  $(,)* })+) => {
        $crate::paste!{$(
        #[test]
        fn [<$name _ $case:snake:lower>]() {
            let (expression, want) = $crate::__prep_bench_or_test!($func, $args, $(Ok($crate::Value::from($ok)))? $(Err($err.to_owned()))?);
            let mut compiler_state = $crate::state::Compiler::default();
            let mut runtime_state = $crate::state::Runtime::default();
            let mut target: $crate::Value = ::std::collections::BTreeMap::default().into();
            let mut ctx = $crate::Context::new(&mut target, &mut runtime_state);


            let got_value = expression.resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got_value, want);

            let got_tdef = expression.type_def(&compiler_state);

            assert_eq!(got_tdef, $tdef);
        }
        )+}
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __prep_bench_or_test {
    ($func:path, $args:expr, $want:expr) => {{
        ($func.compile($args.into()).unwrap(), $want)
    }};
}

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
