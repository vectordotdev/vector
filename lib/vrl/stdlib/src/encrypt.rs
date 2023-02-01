use ::value::Value;
use aes::cipher::{
    block_padding::{AnsiX923, Iso10126, Iso7816, Pkcs7},
    generic_array::GenericArray,
    AsyncStreamCipher, BlockEncryptMut, KeyIvInit, StreamCipher,
};
use cfb_mode::Encryptor as Cfb;
use ctr::Ctr64LE;
use ofb::Ofb;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

type Aes128Cbc = cbc::Encryptor<aes::Aes128>;
type Aes192Cbc = cbc::Encryptor<aes::Aes192>;
type Aes256Cbc = cbc::Encryptor<aes::Aes256>;

pub(crate) fn get_key_bytes<const N: usize>(key: Value) -> Result<[u8; N]> {
    let bytes = key.try_bytes()?;
    if bytes.len() != N {
        return Err(format!(
            "Invalid key size. Expected {} bytes. Found {} bytes",
            N,
            bytes.len()
        )
        .into());
    }

    // This cannot fail since the length was already checked
    Ok(bytes.as_ref().try_into().unwrap())
}

pub(crate) fn get_iv_bytes<const N: usize>(iv: Value) -> Result<[u8; N]> {
    let bytes = iv.try_bytes()?;
    if bytes.len() != N {
        return Err(format!(
            "Invalid iv size. Expected {} bytes. Found {} bytes",
            N,
            bytes.len()
        )
        .into());
    }

    // This cannot fail since the length was already checked
    Ok(bytes.as_ref().try_into().unwrap())
}

macro_rules! encrypt {
    ($algorithm:ty, $plaintext:expr, $key:expr, $iv:expr) => {{
        let mut buffer = vec![0; $plaintext.len()];
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .encrypt_b2b($plaintext.as_ref(), buffer.as_mut())
        .expect("key/iv sizes were already checked");
        buffer
    }};
}

macro_rules! encrypt_padded {
    ($algorithm:ty, $padding:ty, $plaintext:expr, $key:expr, $iv:expr) => {{
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .encrypt_padded_vec_mut::<$padding>($plaintext.as_ref())
    }};
}

macro_rules! encrypt_keystream {
    ($algorithm:ty, $plaintext:expr, $key:expr, $iv:expr) => {{
        let mut buffer = vec![0; $plaintext.len()];
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .apply_keystream_b2b($plaintext.as_ref(), buffer.as_mut())
        .expect("key/iv sizes were already checked");
        buffer
    }};
}

pub(crate) fn is_valid_algorithm(algorithm: Value) -> bool {
    matches!(
        algorithm
            .try_bytes_utf8_lossy()
            .expect("already checked type")
            .as_ref()
            .to_uppercase()
            .as_str(),
        "AES-256-CFB"
            | "AES-192-CFB"
            | "AES-128-CFB"
            | "AES-256-OFB"
            | "AES-192-OFB"
            | "AES-128-OFB"
            | "AES-256-CTR"
            | "AES-192-CTR"
            | "AES-128-CTR"
            | "AES-256-CBC-PKCS7"
            | "AES-192-CBC-PKCS7"
            | "AES-128-CBC-PKCS7"
            | "AES-256-CBC-ANSIX923"
            | "AES-192-CBC-ANSIX923"
            | "AES-128-CBC-ANSIX923"
            | "AES-256-CBC-ISO7816"
            | "AES-192-CBC-ISO7816"
            | "AES-128-CBC-ISO7816"
            | "AES-256-CBC-ISO10126"
            | "AES-192-CBC-ISO10126"
            | "AES-128-CBC-ISO10126"
    )
}

fn encrypt(plaintext: Value, algorithm: Value, key: Value, iv: Value) -> Resolved {
    let plaintext = plaintext.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();
    let ciphertext = match algorithm.as_str() {
        "AES-256-CFB" => encrypt!(Cfb::<aes::Aes256>, plaintext, key, iv),
        "AES-192-CFB" => encrypt!(Cfb::<aes::Aes192>, plaintext, key, iv),
        "AES-128-CFB" => encrypt!(Cfb::<aes::Aes128>, plaintext, key, iv),
        "AES-256-OFB" => encrypt_keystream!(Ofb::<aes::Aes256>, plaintext, key, iv),
        "AES-192-OFB" => encrypt_keystream!(Ofb::<aes::Aes192>, plaintext, key, iv),
        "AES-128-OFB" => encrypt_keystream!(Ofb::<aes::Aes128>, plaintext, key, iv),
        "AES-256-CTR" => encrypt_keystream!(Ctr64LE::<aes::Aes256>, plaintext, key, iv),
        "AES-192-CTR" => encrypt_keystream!(Ctr64LE::<aes::Aes192>, plaintext, key, iv),
        "AES-128-CTR" => encrypt_keystream!(Ctr64LE::<aes::Aes128>, plaintext, key, iv),
        "AES-256-CBC-PKCS7" => encrypt_padded!(Aes256Cbc, Pkcs7, plaintext, key, iv),
        "AES-192-CBC-PKCS7" => encrypt_padded!(Aes192Cbc, Pkcs7, plaintext, key, iv),
        "AES-128-CBC-PKCS7" => encrypt_padded!(Aes128Cbc, Pkcs7, plaintext, key, iv),
        "AES-256-CBC-ANSIX923" => encrypt_padded!(Aes256Cbc, AnsiX923, plaintext, key, iv),
        "AES-192-CBC-ANSIX923" => encrypt_padded!(Aes192Cbc, AnsiX923, plaintext, key, iv),
        "AES-128-CBC-ANSIX923" => encrypt_padded!(Aes128Cbc, AnsiX923, plaintext, key, iv),
        "AES-256-CBC-ISO7816" => encrypt_padded!(Aes256Cbc, Iso7816, plaintext, key, iv),
        "AES-192-CBC-ISO7816" => encrypt_padded!(Aes192Cbc, Iso7816, plaintext, key, iv),
        "AES-128-CBC-ISO7816" => encrypt_padded!(Aes128Cbc, Iso7816, plaintext, key, iv),
        "AES-256-CBC-ISO10126" => encrypt_padded!(Aes256Cbc, Iso10126, plaintext, key, iv),
        "AES-192-CBC-ISO10126" => encrypt_padded!(Aes192Cbc, Iso10126, plaintext, key, iv),
        "AES-128-CBC-ISO10126" => encrypt_padded!(Aes128Cbc, Iso10126, plaintext, key, iv),
        other => return Err(format!("Invalid algorithm: {other}").into()),
    };

    Ok(Value::Bytes(Bytes::from(ciphertext)))
}

#[derive(Clone, Copy, Debug)]
pub struct Encrypt;

impl Function for Encrypt {
    fn identifier(&self) -> &'static str {
        "encrypt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "plaintext",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "algorithm",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "iv",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "encrypt AES-256-CFB",
            source: r#"encode_base64(encrypt!("data", "AES-256-CFB", key: "01234567890123456789012345678912", iv: "0123456789012345"))"#,
            result: Ok("c/dIOA=="),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let plaintext = arguments.required("plaintext");
        let algorithm = arguments.required("algorithm");
        let key = arguments.required("key");
        let iv = arguments.required("iv");

        if let Some(algorithm) = algorithm.as_value() {
            if !is_valid_algorithm(algorithm.clone()) {
                return Err(vrl::function::Error::InvalidArgument {
                    keyword: "algorithm",
                    value: algorithm,
                    error: "Invalid algorithm",
                }
                .into());
            }
        }

        Ok(EncryptFn {
            plaintext,
            algorithm,
            key,
            iv,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
struct EncryptFn {
    plaintext: Box<dyn Expression>,
    algorithm: Box<dyn Expression>,
    key: Box<dyn Expression>,
    iv: Box<dyn Expression>,
}

impl FunctionExpression for EncryptFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let plaintext = self.plaintext.resolve(ctx)?;
        let algorithm = self.algorithm.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;
        let iv = self.iv.resolve(ctx)?;
        encrypt(plaintext, algorithm, key, iv)
    }

    fn type_def(&self, _state: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        encrypt => Encrypt;

        aes_256_cfb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CFB", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xd13\x92\x81\x9a^\x0e=<\x88\xdc\xe7/:]\x90\x08S\x84q")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_cfb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CFB", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"U\xbd6\xdbZ\xbfa}&8\xebog\x19\x99xE\xffL\xf1")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_cfb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CFB", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfd\xf9\xef\x1f@e\xef\xd0Z\xc3\x0c'\xad]\x0e\xd2\x0bZK4")),
            tdef: TypeDef::bytes().fallible(),
        }
        aes_256_ofb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-OFB", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xd13\x92\x81\x9a^\x0e=<\x88\xdc\xe7/:]\x90\xfe(\x89k")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_ofb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-OFB", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"U\xbd6\xdbZ\xbfa}&8\xebog\x19\x99x\xe4\xf4J\x1f")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_ofb {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-OFB", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfd\xf9\xef\x1f@e\xef\xd0Z\xc3\x0c'\xad]\x0e\xd2Qi\xe9\xf4")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_256_ctr {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CTR", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xd13\x92\x81\x9a^\x0e=<\x88\xdc\xe7/:]\x90\x9a\x99\xa7\xb6")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_ctr {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CTR", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"U\xbd6\xdbZ\xbfa}&8\xebog\x19\x99x\x88\xb69n")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_ctr {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CTR", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfd\xf9\xef\x1f@e\xef\xd0Z\xc3\x0c'\xad]\x0e\xd2v\x04\x05\xee")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_256_cbc_pkcs7 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CBC-PKCS7", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x80-9O\x1c\xf1,R\x02\xa0\x0e\x17G\xd8B\xf4\xf9q\xf3\x0c\xcaK\x03h\xbc\xb2\xe8vU\x12\x10\xb3")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_cbc_pkcs7 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CBC-PKCS7", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfaG\x97OVj\xd4\xf5\x80\x1c\x9f}\xac,:t\xfb\xca\xe5\xf1\x8c\x08\xed\\\xf5\xff\xef\xf8\xe9\n\x9c*")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_cbc_pkcs7 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CBC-PKCS7", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x94R\xb5\xfeE\xd9)N1\xd3\xfe\xe66E\x05\x9ch\xae\xf6\x82\rD\xfdH\xd3T8n\xa7\xec\x98W")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_256_cbc_ansix923 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CBC-ANSIX923", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x80-9O\x1c\xf1,R\x02\xa0\x0e\x17G\xd8B\xf4\xd9vj\x15\n&\x92\xea\xee\x03 \xeb\x9e\x8f\x97\x90")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_cbc_ansix923 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CBC-ANSIX923", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfaG\x97OVj\xd4\xf5\x80\x1c\x9f}\xac,:t\xbc\xaf\xbd\xdf0\x10\xdc\xe7\x10Lk\xe4\x03;\xa2\xf5")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_cbc_ansix923 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CBC-ANSIX923", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x94R\xb5\xfeE\xd9)N1\xd3\xfe\xe66E\x05\x9cEnq\x0f9\x02\xfe/T\x0f\xc5\x18r\x95\"\xe3")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_256_cbc_iso7816 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CBC-ISO7816", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x80-9O\x1c\xf1,R\x02\xa0\x0e\x17G\xd8B\xf4\x84\x12\xeb\xe6i\xef\xbcN\xe85\\HnV\xb2\x92")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_cbc_iso7816 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CBC-ISO7816", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfaG\x97OVj\xd4\xf5\x80\x1c\x9f}\xac,:t%lnCr;N\xbcq\xfeE\xb4\x83a \x9b")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_cbc_iso7816 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CBC-ISO7816", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x94R\xb5\xfeE\xd9)N1\xd3\xfe\xe66E\x05\x9cWp\xcfu\xba\x86\x01Q\x9fw\x8f\xf2\x12\xba\x9b0")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_256_cbc_iso10126 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-256-CBC-ISO10126", key: "32_bytes_xxxxxxxxxxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x80-9O\x1c\xf1,R\x02\xa0\x0e\x17G\xd8B\xf4\xf9q\xf3\x0c\xcaK\x03h\xbc\xb2\xe8vU\x12\x10\xb3")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_192_cbc_iso10126 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-192-CBC-ISO10126", key: "24_bytes_xxxxxxxxxxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\xfaG\x97OVj\xd4\xf5\x80\x1c\x9f}\xac,:t\xfb\xca\xe5\xf1\x8c\x08\xed\\\xf5\xff\xef\xf8\xe9\n\x9c*")),
            tdef: TypeDef::bytes().fallible(),
        }

        aes_128_cbc_iso10126 {
            args: func_args![plaintext: value!("morethan1blockofdata"), algorithm: "AES-128-CBC-ISO10126", key: "16_bytes_xxxxxxx", iv: "16_bytes_xxxxxxx"],
            want: Ok(value!(b"\x94R\xb5\xfeE\xd9)N1\xd3\xfe\xe66E\x05\x9ch\xae\xf6\x82\rD\xfdH\xd3T8n\xa7\xec\x98W")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
