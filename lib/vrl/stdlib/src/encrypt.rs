use aes::cipher::block_padding::{AnsiX923, Iso10126, Iso7816, Pkcs7};
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{AsyncStreamCipher, BlockEncrypt, BlockEncryptMut, KeyInit, KeySizeUser};
use std::str::Split;
use vrl::prelude::expression::Expr;
use vrl::prelude::value::Error;
use vrl::prelude::*;

use aes::cipher::KeyIvInit;
use aes::cipher::StreamCipher;
use aes::Aes256;
use bytes::BytesMut;

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

pub(crate) fn get_iv_bytes<const N: usize>(iv: Option<Value>) -> Result<[u8; N]> {
    let iv = match iv {
        Some(iv) => iv,
        None => return Err(format!("iv parameter is required",).into()),
    };

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
        .unwrap();
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
        .unwrap();
        buffer
    }};
}

fn encrypt(plaintext: Value, algorithm: Value, key: Value, iv: Option<Value>) -> Resolved {
    let mut plaintext = plaintext.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();
    let ciphertext = match algorithm.as_str() {
        "AES-256-CFB" => encrypt!(cfb_mode::Encryptor::<aes::Aes256>, plaintext, key, iv),
        "AES-192-CFB" => encrypt!(cfb_mode::Encryptor::<aes::Aes192>, plaintext, key, iv),
        "AES-128-CFB" => encrypt!(cfb_mode::Encryptor::<aes::Aes128>, plaintext, key, iv),
        "AES-256-OFB" => encrypt_keystream!(ofb::Ofb::<aes::Aes256>, plaintext, key, iv),
        "AES-192-OFB" => encrypt_keystream!(ofb::Ofb::<aes::Aes192>, plaintext, key, iv),
        "AES-128-OFB" => encrypt_keystream!(ofb::Ofb::<aes::Aes128>, plaintext, key, iv),
        "AES-256-CTR" => encrypt_keystream!(ctr::Ctr64LE::<aes::Aes256>, plaintext, key, iv),
        "AES-192-CTR" => encrypt_keystream!(ctr::Ctr64LE::<aes::Aes192>, plaintext, key, iv),
        "AES-128-CTR" => encrypt_keystream!(ctr::Ctr64LE::<aes::Aes128>, plaintext, key, iv),
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
        other => return Err(format!("Invalid algorithm: {}", other).into()),
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
                required: false,
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
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let plaintext = arguments.required("plaintext");
        let algorithm = arguments.required("algorithm");
        let key = arguments.required("key");
        let iv = arguments.optional("iv");

        Ok(Box::new(EncryptFn {
            plaintext,
            algorithm,
            key,
            iv,
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let plaintext = args.required("plaintext");
        let algorithm = args.required("algorithm");
        let key = args.required("key");
        let iv = args.optional("iv");
        encrypt(plaintext, algorithm, key, iv)
    }
}

#[derive(Debug, Clone)]
struct EncryptFn {
    plaintext: Box<dyn Expression>,
    algorithm: Box<dyn Expression>,
    key: Box<dyn Expression>,
    iv: Option<Box<dyn Expression>>,
}

impl Expression for EncryptFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let plaintext = self.plaintext.resolve(ctx)?;
        let algorithm = self.algorithm.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;

        let iv = match &self.iv {
            None => None,
            Some(iv) => Some(iv.resolve(ctx)?),
        };

        encrypt(plaintext, algorithm, key, iv)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
