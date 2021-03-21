/// An immutable string
///
/// This `String` differs from the basic Rust `String` in that it is not mutable
/// -- once it's created it's created -- and as a result it cannot change
/// size. It consumes 2*usize words overhead where `std::string::String` has
/// 3*usize.
pub type String = Box<str>;
