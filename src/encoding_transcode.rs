use bytes::{Bytes, BytesMut};
use encoding_rs::{CoderResult, Encoding};

// FIXME add tests

const BUFFER_SIZE: usize = 4096;

/// Helps transcoding from specified encoding to utf8
pub struct Decoder {
    buffer: [u8; BUFFER_SIZE],
    output: BytesMut,
    inner: encoding_rs::Decoder,
}

impl Decoder {
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            buffer: [0; BUFFER_SIZE],
            output: BytesMut::new(),
            // if input starts with byte order mark (BOM) for the given encoding,
            // we want to remove those bytes (useful if the input is coming from
            // files, for example)
            inner: encoding.new_decoder_with_bom_removal(),
        }
    }

    pub fn to_utf8(&mut self, input: Bytes) -> Bytes {
        let mut total_read_from_input = 0;
        let mut total_had_errors = false;

        loop {
            let (result, read, written, had_errors) = self.inner.decode_to_utf8(
                &input[total_read_from_input..],
                &mut self.buffer,
                false, // not last (since we are processing a continous stream)
            );

            total_read_from_input += read;
            total_had_errors |= had_errors;

            self.output.extend_from_slice(&self.buffer[..written]);

            match result {
                CoderResult::InputEmpty => {
                    // we have consumed all of the given input so we are done!
                    break;
                }
                CoderResult::OutputFull => {
                    continue;
                }
            }
        }

        if total_had_errors {
            warn!(
                message = "Replaced malformed sequences with replacement character while decoding to utf8",
                from_encoding = %self.inner.encoding().name()
            );
        }

        self.output.split().freeze()
    }
}

/// Helps transcoding to specified encoding from utf8
pub struct Encoder {
    buffer: [u8; BUFFER_SIZE],
    output: BytesMut,
    inner: encoding_rs::Encoder,
    // since encoding_rs does not have encoders for utf16, need to track this
    // https://docs.rs/encoding_rs/0.8.26/encoding_rs/index.html#utf-16le-utf-16be-and-unicode-encoding-schemes
    utf16_encoding: Option<UTF16Encoding>,
}

#[derive(Debug, Clone, Copy)]
enum UTF16Encoding {
    LE, // little-endian
    BE, // big-endian
}

impl Encoder {
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            buffer: [0; BUFFER_SIZE],
            output: BytesMut::new(),
            inner: encoding.new_encoder(),
            utf16_encoding: Self::get_utf16_encoding(encoding),
        }
    }

    fn get_utf16_encoding(encoding: &'static Encoding) -> Option<UTF16Encoding> {
        match encoding.name() {
            "UTF-16LE" => Some(UTF16Encoding::LE),
            "UTF-16BE" => Some(UTF16Encoding::BE),
            _ => None,
        }
    }

    fn from_utf8_to_utf16(&mut self, input: &str, variant: &UTF16Encoding) -> Bytes {
        let mut utf16_values = input.encode_utf16();

        let to_bytes_func = match variant {
            UTF16Encoding::LE => u16::to_le_bytes,
            UTF16Encoding::BE => u16::to_be_bytes,
        };

        while let Some(v) = utf16_values.next() {
            self.output.extend_from_slice(&to_bytes_func(v));
        }

        self.output.split().freeze()
    }

    pub fn from_utf8(&mut self, input: &str) -> Bytes {
        // alternate logic if the encoder is for a utf-16 encoding variant
        if let Some(variant) = self.utf16_encoding {
            return self.from_utf8_to_utf16(input, &variant);
        }

        let mut total_read_from_input = 0;
        let mut total_had_errors = false;

        loop {
            let (result, read, written, had_errors) = self.inner.encode_from_utf8(
                &input[total_read_from_input..],
                &mut self.buffer,
                false, // not last (since we are processing a continous stream)
            );

            total_read_from_input += read;
            total_had_errors |= had_errors;

            self.output.extend_from_slice(&self.buffer[..written]);

            match result {
                CoderResult::InputEmpty => {
                    // we have consumed all of the given input so we are done!
                    break;
                }
                CoderResult::OutputFull => {
                    continue;
                }
            }
        }

        if total_had_errors {
            warn!(
                message = "Replaced unmappable characters with numeric character references while encoding from utf8",
                to_encoding = %self.inner.encoding().name()
            );
        }

        self.output.split().freeze()
    }
}
