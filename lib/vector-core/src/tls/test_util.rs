//! Certificate helpers shared by the TLS tests.

use openssl::{
    asn1::Asn1Time,
    bn::{BigNum, MsbOption},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    rsa::Rsa,
    x509::{X509, X509NameBuilder, X509Ref},
};

/// Generates a self-signed certificate, valid for a day, with the given common name, and its key.
pub(super) fn self_signed(common_name: &str) -> (X509, PKey<Private>) {
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();

    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", common_name).unwrap();
    let name = name.build();

    let mut serial = BigNum::new().unwrap();
    serial.rand(128, MsbOption::MAYBE_ZERO, false).unwrap();

    let mut builder = X509::builder().unwrap();
    builder.set_version(2).unwrap();
    builder
        .set_serial_number(&serial.to_asn1_integer().unwrap())
        .unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&key).unwrap();
    builder
        .set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::days_from_now(1).unwrap())
        .unwrap();
    builder.sign(&key, MessageDigest::sha256()).unwrap();

    (builder.build(), key)
}

/// Returns the common name of `cert`'s subject.
pub(super) fn common_name(cert: &X509Ref) -> String {
    cert.subject_name()
        .entries_by_nid(Nid::COMMONNAME)
        .next()
        .unwrap()
        .data()
        .to_string()
        .unwrap()
}
