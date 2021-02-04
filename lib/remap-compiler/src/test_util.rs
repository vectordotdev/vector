#[macro_export]
macro_rules! test_type_def {
    ($($name:ident { expr: $expr:expr, want: $def:expr, })+) => {
        mod type_def {
            use super::*;

            $(
                #[test]
                fn $name() {
                    let mut state = $crate::state::Compiler::default();
                    let expression: Box<dyn $crate::Expression> = Box::new($expr(&mut state));

                    assert_eq!(expression.type_def(&state), $def);
                }
            )+
        }
    };
}
