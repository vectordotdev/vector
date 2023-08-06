macro_rules! fatal {
    ($($arg:tt)*) => {{
        use owo_colors::OwoColorize;
        eprintln!(
            "{}",
            format!($($arg)*)
                .if_supports_color(owo_colors::Stream::Stderr, |text| text.bright_red())
        );
        std::process::exit(1);
    }};
}

macro_rules! define_display_macro {
    // https://github.com/rust-lang/rust/issues/35853#issuecomment-415993963
    // https://github.com/rust-lang/rust/issues/83527#issuecomment-1281176235
    ($name:ident, $level:ident, $style:ident, $d:tt) => (
        #[allow(unused_macros)]
        macro_rules! $name {
            ($d($d arg:tt)*) => {{
                use owo_colors::OwoColorize;
                if log::Level::$level <= *$crate::app::verbosity() {
                    eprintln!(
                        "{}",
                        format!($d($d arg)*)
                            .if_supports_color(owo_colors::Stream::Stderr, |text| text.$style())
                    );
                }
            }};
        }
    );
}

define_display_macro!(trace, Trace, underline, $);
define_display_macro!(debug, Debug, italic, $);
define_display_macro!(info, Info, bold, $);
define_display_macro!(success, Info, bright_cyan, $);
define_display_macro!(waiting, Info, bright_magenta, $);
define_display_macro!(warn, Warn, bright_yellow, $);
define_display_macro!(error, Error, bright_red, $);
