use std::error;

use bytes::{Buf, BufMut};

/// Converts back and forth between user-friendly metadata types and the on-disk integer representation.
pub trait AsMetadata: Sized {
    /// Converts this metadata value into its integer representation.
    fn into_u32(self) -> u32;

    /// Converts an integer repentation of metadata into its real type, if possible.
    ///
    /// If the given integer does not represent a valid representation of the given metadata type,
    /// possibly due to including bits not valid for the type, and so on, then `None` will be
    /// returned.  Otherwise, `Some(Self)` will be returned.
    fn from_u32(value: u32) -> Option<Self>;
}

impl AsMetadata for () {
    fn into_u32(self) -> u32 {
        0
    }

    fn from_u32(_: u32) -> Option<Self> {
        Some(())
    }
}

/// An object that can encode and decode itself to and from a buffer.
///
/// # Metadata
///
/// While an encoding implementation is typically fixed i.e. `MyJsonEncoderType` only encodes and
/// decodes JSON, we want to provide the ability to change encodings and schemas over time without
/// fundamentally changing all of the code in the buffer implementations.
///
/// We provide the ability to express "metadata" about the encoding implementation such that any
/// relevant information can be included alongside the encoded object, and then passed back when
/// decoding is required.
///
/// ## Implementation
///
/// As designed, an implementor would define a primary encoding scheme, schema version, and so on,
/// that matched how an object would be encoded.  This is acquired from `get_metadata` by code tht
/// depends on `Encodable` and will be stored alongside the encoded object.  When the encoded object
/// is later read back, and the caller wants to decode it, they would also read the metadata and do
/// two things: check that the metadata is still valid for this implementation by calling
/// `can_decode` and then pass it along to the `decode` call itself.
///
/// ## Verifying ability to decode
///
/// Calling `can_decode` first allows callers to check if the encoding implementation supports the
/// parameters used to encode the given object, which provides a means to allow for versioning,
/// schema evolution, and more.  Practically speaking, an implementation might bump the version of
/// its schema, but still support the old version for some time, and so `can_decode` might simply
/// check that the metadata represents the current version of the schema, or the last version, but
/// no other versions would be allowed.  When the old version of the schema was finally removed and
/// no longer supported, `can_decode` would no longer say it could decode any object whose metadata
/// referenced that old version.
///
/// The `can_decode` method is provided separately, instead of being lumped together in the `decode`
/// call, as a means to distinguish a lack of decoding support for a given metadata from a general
/// decoding failure.
///
/// ## Metadata-aware decoding
///
/// Likewise, the call to `decode` is given the metadata that was stored with the encoded object so
/// that it knows exactly what parameters were originally used and thus how it needs approach
/// decoding the object.
///
/// ## Metadata format and meaning
///
/// Ostensibly, the metadata would represent either some sort of numeric version identifier, or
/// could be used in a bitflags-style fashion, where each bit represents a particular piece of
/// information: encoding type, schema version, whether specific information is present in the
/// encoded object, and so on.
pub trait Encodable: Sized {
    type Metadata: AsMetadata + Copy;
    type EncodeError: error::Error + Send + Sync + 'static;
    type DecodeError: error::Error + Send + Sync + 'static;

    /// Gets the version metadata associated with this encoding scheme.
    ///
    /// The value provided is ostensibly used as a bitfield-esque container, or potentially as a raw
    /// numeric version identifier, that identifies how a value was encoded, as well as any other
    /// information that may be necessary to successfully decode it.
    fn get_metadata() -> Self::Metadata;

    /// Whether or not this encoding scheme can understand and successfully decode a value based on
    /// the given version metadata that was bundled with the value.
    fn can_decode(metadata: Self::Metadata) -> bool;

    /// Attempts to encode this value into the given buffer.
    ///
    /// # Errors
    ///
    /// If there is an error while attempting to encode this value, an error variant will be
    /// returned describing the error.
    ///
    /// Practically speaking, based on the API, encoding errors should generally only occur if there
    /// is insufficient space in the buffer to fully encode this value.  However, this is not
    /// guaranteed.
    fn encode<B: BufMut>(self, buffer: &mut B) -> Result<(), Self::EncodeError>;

    /// Gets the encoded size, in bytes, of this value, if available.
    ///
    /// Not all types can know ahead of time how many bytes they will occupy when encoded, hence the
    /// fallibility of this method.
    fn encoded_size(&self) -> Option<usize> {
        None
    }

    /// Attempts to decode an instance of this type from the given buffer and metadata.
    ///
    /// # Errors
    ///
    /// If there is an error while attempting to decode a value from the given buffer, or the given
    /// metadata is not valid for the implementation, an error variant will be returned describing
    /// the error.
    fn decode<B: Buf>(metadata: Self::Metadata, buffer: B) -> Result<Self, Self::DecodeError>;
}

/// An object that can encode and decode itself to and from a buffer, with a fixed representation.
///
/// This trait is a companion trait to `Encodable` that provides a blanket implementation of
/// `Encodable` that does not use or care about encoding metadata.  It fulfills the necessary
/// methods to work in `Encodable` contexts without requiring any of the boilerplate.
///
/// ## Warning
///
/// You should _not_ typically use this trait unless you're trying to implement `Encodable` for
/// testing purposes where you won't be dealing with a need for versioning payloads, etc.
///
/// For any types that will potentially be encoded in real use cases, `Encodable` should be
/// preferred as it requires an upfront decision to be made about metadata and how it's dealt with.
pub trait FixedEncodable: Sized {
    type EncodeError: error::Error + Send + Sync + 'static;
    type DecodeError: error::Error + Send + Sync + 'static;

    /// Attempts to encode this value into the given buffer.
    ///
    /// # Errors
    ///
    /// If there is an error while attempting to encode this value, an error variant will be
    /// returned describing the error.
    ///
    /// Practically speaking, based on the API, encoding errors should generally only occur if there
    /// is insufficient space in the buffer to fully encode this value.  However, this is not
    /// guaranteed.
    fn encode<B: BufMut>(self, buffer: &mut B) -> Result<(), Self::EncodeError>;

    /// Gets the encoded size, in bytes, of this value, if available.
    ///
    /// Not all types can know ahead of time how many bytes they will occupy when encoded, hence the
    /// fallibility of this method.
    fn encoded_size(&self) -> Option<usize> {
        None
    }

    /// Attempts to decode an instance of this type from the given buffer.
    ///
    /// # Errors
    ///
    /// If there is an error while attempting to decode a value from the given buffer, an error
    /// variant will be returned describing the error.
    fn decode<B: Buf>(buffer: B) -> Result<Self, Self::DecodeError>;
}

impl<T: FixedEncodable> Encodable for T {
    type Metadata = ();
    type EncodeError = <T as FixedEncodable>::EncodeError;
    type DecodeError = <T as FixedEncodable>::DecodeError;

    fn get_metadata() -> Self::Metadata {}

    fn can_decode(_: Self::Metadata) -> bool {
        true
    }

    fn encode<B: BufMut>(self, buffer: &mut B) -> Result<(), Self::EncodeError> {
        FixedEncodable::encode(self, buffer)
    }

    fn encoded_size(&self) -> Option<usize> {
        FixedEncodable::encoded_size(self)
    }

    fn decode<B: Buf>(_: Self::Metadata, buffer: B) -> Result<Self, Self::DecodeError> {
        <Self as FixedEncodable>::decode(buffer)
    }
}
