use std::{convert::TryInto, mem};

#[derive(Copy, Clone, Debug)]
pub(crate) struct Key(pub usize);

impl db_key::Key for Key {
    fn from_u8(key: &[u8]) -> Self {
        let bytes: [u8; mem::size_of::<usize>()] =
            key.try_into().expect("Key should be the right size");

        Self(usize::from_be_bytes(bytes))
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        let bytes = self.0.to_be_bytes();
        f(&bytes)
    }
}
