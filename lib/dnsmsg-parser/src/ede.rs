use hickory_proto::{
    error::ProtoResult,
    serialize::binary::{BinDecodable, BinDecoder, BinEncodable, BinEncoder},
};

pub const EDE_OPTION_CODE: u16 = 15u16;

#[derive(Debug, Clone)]
pub struct EDE {
    info_code: u16,
    extra_text: Option<String>,
}

impl EDE {
    pub fn new(info_code: u16, extra_text: Option<String>) -> Self {
        Self {
            info_code,
            extra_text,
        }
    }

    // https://www.iana.org/assignments/dns-parameters/dns-parameters.xhtml#extended-dns-error-codes
    pub fn purpose(&self) -> Option<&str> {
        match self.info_code {
            0 => Some("Other Error"),
            1 => Some("Unsupported DNSKEY Algorithm"),
            2 => Some("Unsupported DS Digest Type"),
            3 => Some("Stale Answer"),
            4 => Some("Forged Answer"),
            5 => Some("DNSSEC Indeterminate"),
            6 => Some("DNSSEC Bogus"),
            7 => Some("Signature Expired"),
            8 => Some("Signature Not Yet Valid"),
            9 => Some("DNSKEY Missing"),
            10 => Some("RRSIGs Missing"),
            11 => Some("No Zone Key Bit Set"),
            12 => Some("NSEC Missing"),
            13 => Some("Cached Error"),
            14 => Some("Not Ready"),
            15 => Some("Blocked"),
            16 => Some("Censored"),
            17 => Some("Filtered"),
            18 => Some("Prohibited"),
            19 => Some("Stale NXDomain Answer"),
            20 => Some("Not Authoritative"),
            21 => Some("Not Supported"),
            22 => Some("No Reachable Authority"),
            23 => Some("Network Error"),
            24 => Some("Invalid Data"),
            25 => Some("Signature Expired before Valid"),
            26 => Some("Too Early"),
            27 => Some("Unsupported NSEC3 Iterations Value"),
            28 => Some("Unable to conform to policy"),
            29 => Some("Synthesized"),
            _ => None,
        }
    }

    pub fn info_code(&self) -> u16 {
        self.info_code
    }

    pub fn extra_text(&self) -> Option<String> {
        self.extra_text.clone()
    }
}

impl BinEncodable for EDE {
    fn emit(&self, encoder: &mut BinEncoder<'_>) -> ProtoResult<()> {
        encoder.emit_u16(self.info_code)?;
        if let Some(extra_text) = &self.extra_text {
            encoder.emit_vec(extra_text.as_bytes())?;
        }
        Ok(())
    }
}

impl<'a> BinDecodable<'a> for EDE {
    fn read(decoder: &mut BinDecoder<'a>) -> ProtoResult<Self> {
        let info_code = decoder.read_u16()?.unverified();
        let extra_text = if decoder.is_empty() {
            None
        } else {
            Some(String::from_utf8(
                decoder.read_vec(decoder.len())?.unverified(),
            )?)
        };
        Ok(Self {
            info_code,
            extra_text,
        })
    }
}
