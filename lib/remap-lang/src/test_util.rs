#[cfg(test)]
#[macro_export]
macro_rules! test_type_check {
    ($($name:ident { expr: $expr:expr, def: $def:expr, })+) => {
        mod type_check {
            use super::*;

            $(
                #[test]
                fn $name() {
                    let mut state = CompilerState::default();
                    let expression: Box<dyn Expression> = Box::new($expr(&mut state));

                    assert_eq!(expression.type_check(&state), $def);
                }
            )+
        }
    };
}
