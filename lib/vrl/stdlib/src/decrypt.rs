use aes::cipher::block_padding::{AnsiX923, Iso10126, Iso7816, Pkcs7};
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{AsyncStreamCipher, BlockDecryptMut, KeyInit, StreamCipher};
use std::str::Split;
use vrl::prelude::expression::Expr;
use vrl::prelude::value::Error;
use vrl::prelude::*;

use crate::encrypt::{get_iv_bytes, get_key_bytes};
use aes::cipher::KeyIvInit;
use aes::Aes256;
use bytes::BytesMut;

type Aes128Cbc = cbc::Decryptor<aes::Aes128>;
type Aes192Cbc = cbc::Decryptor<aes::Aes192>;
type Aes256Cbc = cbc::Decryptor<aes::Aes256>;

macro_rules! decrypt {
    ($algorithm:ty, $ciphertext:expr, $key:expr, $iv:expr) => {{
        let mut buffer = vec![0; $ciphertext.len()];
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .decrypt_b2b($ciphertext.as_ref(), buffer.as_mut())
        .unwrap();
        buffer
    }};
}

macro_rules! decrypt_padded {
    ($algorithm:ty, $padding:ty, $ciphertext:expr, $key:expr, $iv:expr) => {{
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .decrypt_padded_vec_mut::<$padding>($ciphertext.as_ref())
        .map_err(|_| format!("Invalid input"))?
    }};
}

macro_rules! decrypt_keystream {
    ($algorithm:ty, $ciphertext:expr, $key:expr, $iv:expr) => {{
        let mut buffer = vec![0; $ciphertext.len()];
        <$algorithm>::new(
            &GenericArray::from(get_key_bytes($key)?),
            &GenericArray::from(get_iv_bytes($iv)?),
        )
        .apply_keystream_b2b($ciphertext.as_ref(), buffer.as_mut())
        .unwrap();
        buffer
    }};
}

fn decrypt(ciphertext: Value, algorithm: Value, key: Value, iv: Option<Value>) -> Resolved {
    let mut ciphertext = ciphertext.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();
    let ciphertext = match algorithm.as_str() {
        "AES-256-CFB" => decrypt!(cfb_mode::Decryptor::<aes::Aes256>, ciphertext, key, iv),
        "AES-192-CFB" => decrypt!(cfb_mode::Decryptor::<aes::Aes192>, ciphertext, key, iv),
        "AES-128-CFB" => decrypt!(cfb_mode::Decryptor::<aes::Aes128>, ciphertext, key, iv),
        "AES-256-OFB" => decrypt_keystream!(ofb::Ofb::<aes::Aes256>, ciphertext, key, iv),
        "AES-192-OFB" => decrypt_keystream!(ofb::Ofb::<aes::Aes192>, ciphertext, key, iv),
        "AES-128-OFB" => decrypt_keystream!(ofb::Ofb::<aes::Aes128>, ciphertext, key, iv),
        "AES-256-CTR" => decrypt_keystream!(ctr::Ctr64LE::<aes::Aes256>, ciphertext, key, iv),
        "AES-192-CTR" => decrypt_keystream!(ctr::Ctr64LE::<aes::Aes192>, ciphertext, key, iv),
        "AES-128-CTR" => decrypt_keystream!(ctr::Ctr64LE::<aes::Aes128>, ciphertext, key, iv),
        "AES-256-CBC-PKCS7" => decrypt_padded!(Aes256Cbc, Pkcs7, ciphertext, key, iv),
        "AES-192-CBC-PKCS7" => decrypt_padded!(Aes192Cbc, Pkcs7, ciphertext, key, iv),
        "AES-128-CBC-PKCS7" => decrypt_padded!(Aes128Cbc, Pkcs7, ciphertext, key, iv),
        "AES-256-CBC-ANSIX923" => decrypt_padded!(Aes256Cbc, AnsiX923, ciphertext, key, iv),
        "AES-192-CBC-ANSIX923" => decrypt_padded!(Aes192Cbc, AnsiX923, ciphertext, key, iv),
        "AES-128-CBC-ANSIX923" => decrypt_padded!(Aes128Cbc, AnsiX923, ciphertext, key, iv),
        "AES-256-CBC-ISO7816" => decrypt_padded!(Aes256Cbc, Iso7816, ciphertext, key, iv),
        "AES-192-CBC-ISO7816" => decrypt_padded!(Aes192Cbc, Iso7816, ciphertext, key, iv),
        "AES-128-CBC-ISO7816" => decrypt_padded!(Aes128Cbc, Iso7816, ciphertext, key, iv),
        "AES-256-CBC-ISO10126" => decrypt_padded!(Aes256Cbc, Iso10126, ciphertext, key, iv),
        "AES-192-CBC-ISO10126" => decrypt_padded!(Aes192Cbc, Iso10126, ciphertext, key, iv),
        "AES-128-CBC-ISO10126" => decrypt_padded!(Aes128Cbc, Iso10126, ciphertext, key, iv),
        other => return Err(format!("Invalid algorithm: {}", other).into()),
    };

    Ok(Value::Bytes(Bytes::from(ciphertext)))
}

// fn decrypt(plaintext: Value, algorithm: Value, key: Value, iv: Option<Value>) -> Resolved {
//     let mut plaintext = plaintext.try_bytes()?;
//     let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();
//
//     let ciphertext = match algorithm.as_str() {
//         "AES-256-CFB" => {
//             let mut buffer = vec![0; plaintext.len()];
//             cfb_mode::Decryptor::<aes::Aes256>::new(
//                 &GenericArray::from(get_key_bytes(key)?),
//                 &GenericArray::from(get_iv_bytes(iv)?),
//             )
//             .decrypt_b2b(plaintext.as_ref(), buffer.as_mut())
//             .unwrap();
//             buffer
//         }
//         other => return Err(format!("Invalid algorithm: {}", other).into()),
//     };
//
//     Ok(Value::Bytes(Bytes::from(ciphertext)))
// }

#[derive(Clone, Copy, Debug)]
pub struct Decrypt;

impl Function for Decrypt {
    fn identifier(&self) -> &'static str {
        "decrypt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "ciphertext",
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
            title: "decrypt AES-256-CFB",
            source: r#"decrypt!(decode_base64!("c/dIOA=="), "AES-256-CFB", key: "01234567890123456789012345678912", iv: "0123456789012345")"#,
            result: Ok("data"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let ciphertext = arguments.required("ciphertext");
        let algorithm = arguments.required("algorithm");
        let key = arguments.required("key");
        let iv = arguments.optional("iv");

        Ok(Box::new(DecryptFn {
            ciphertext,
            algorithm,
            key,
            iv,
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let ciphertext = args.required("ciphertext");
        let algorithm = args.required("algorithm");
        let key = args.required("key");
        let iv = args.optional("iv");
        decrypt(ciphertext, algorithm, key, iv)
    }
}

#[derive(Debug, Clone)]
struct DecryptFn {
    ciphertext: Box<dyn Expression>,
    algorithm: Box<dyn Expression>,
    key: Box<dyn Expression>,
    iv: Option<Box<dyn Expression>>,
}

impl Expression for DecryptFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let ciphertext = self.ciphertext.resolve(ctx)?;
        let algorithm = self.algorithm.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;

        let iv = match &self.iv {
            None => None,
            Some(iv) => Some(iv.resolve(ctx)?),
        };

        decrypt(ciphertext, algorithm, key, iv)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
