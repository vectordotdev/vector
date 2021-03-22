/// An immutable string
///
/// This `ImStr` differs from the basic Rust `String` in that it is not mutable
/// -- once it's created it's created -- and as a result it cannot change
/// size. It consumes 2*usize words overhead where `std::string::String` has
/// 3*usize.
///
/// This string type is equivalent to a boxed `str` -- hence the name -- and as
/// such is subject to [interesting
/// optimizations](https://doc.rust-lang.org/std/option/#representation).
pub type ImStr = Box<str>;
