//! The Vector Core Library
//!
//! The Vector Core Library are the foundational pieces needed to make a vector
//! and is not vector with pieces missing. While this library is obviously
//! tailored to the needs of vector it is written in such a way to make
//! experimentation and testing _in the library_ cheap and demonstrative.
//!
//! This library was extracted from the top-level project package, discussed in
//! RFC 7027.

#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(4, 2 + 2)
    }
}
