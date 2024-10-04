use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DecoderBomRemoval {
    pub from_encoding: &'static str,
}

impl InternalEvent for DecoderBomRemoval {
    fn emit(self) {
        trace!(
            message = "Removing initial BOM bytes from the final output while decoding to utf8.",
            from_encoding = %self.from_encoding,
            internal_log_rate_limit = true
        );
        counter!("decoder_bom_removals_total").increment(1);
    }
}

#[derive(Debug)]
pub struct DecoderMalformedReplacement {
    pub from_encoding: &'static str,
}

impl InternalEvent for DecoderMalformedReplacement {
    fn emit(self) {
        warn!(
            message = "Replaced malformed sequences with replacement character while decoding to utf8.",
            from_encoding = %self.from_encoding,
            internal_log_rate_limit = true
        );
        // NOT the actual number of replacements in the output: there's no easy
        // way to get that from the lib we use here (encoding_rs)
        counter!("decoder_malformed_replacement_warnings_total").increment(1);
    }
}

#[derive(Debug)]
pub struct EncoderUnmappableReplacement {
    pub to_encoding: &'static str,
}

impl InternalEvent for EncoderUnmappableReplacement {
    fn emit(self) {
        warn!(
            message = "Replaced unmappable characters with numeric character references while encoding from utf8.",
            to_encoding = %self.to_encoding,
            internal_log_rate_limit = true
        );
        // NOT the actual number of replacements in the output: there's no easy
        // way to get that from the lib we use here (encoding_rs)
        counter!("encoder_unmappable_replacement_warnings_total").increment(1);
    }
}
