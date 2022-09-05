---
date: "2022-05-24"
title: "New encryption functions for VRL"
description: "Encrypt and decrypt data in VRL"
authors: ["jszwedko"]
pr_numbers: []
release: "0.22.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

VRL now contains facilities for encrypting and decrypting field values using [AES][AES] encryption with user
provided keys via new [`encrypt`][encrypt] and [`decrypt`][decrypt] functions. A [`random_bytes`][random_bytes] function
was added to make it easy to generate initialization vectors for the [`encrypt`][encrypt] function.

These can be useful to:

- Decrypt encrypted data that is sent to Vector for processing
- Encrypting sensitive fields in Vector for storage in an external service that can be decrypted on-demand if the
  original value is required

## Encrypting data

To encrypt data in VRL with an encryption key you can provide an encryption key. Typically the key is injected via an
environment variable.

An example with input:

```json
{ "plaintext": "super secret message" }
```

```coffeescript
# The `key` is typically a raw set of bytes matching the expected length for the algorithm so it is common to base64
# encode it for injection and decode in VRL
key = decode_base64!(get_env_var!("KEY")) # with $KEY set to "c2VjcmV0X19faHVudGVyMg==" in this example

# we store the iv on the event to use as-needed for decryption
.iv = "1234567890123456" # typically you would call random_bytes(<num bytes expected by algorithm>)

encrypted_message = encrypt!(plaintext, "AES-128-CBC-PKCS7", key, iv: iv)

# Often you will want to encode the result of the encryption as base64 so it can be represented as a string
.encrypted_message = encode_base64(encrypted_message)

# delete original
del(.plaintext)
```

The result will be:

```json
{ "encrypted_message": "jYn3wFE2ajfd/VpDE/SrLO5+DknxB3hqgjH5+hpnSu4=" }
```

## Decrypting data

To decrypt data in VRL with an encryption key you can provide an encryption key (typically injected via an environment
variable).

An example with input (from above encryption example):

```json
{ "encrypted_message": "jYn3wFE2ajfd/VpDE/SrLO5+DknxB3hqgjH5+hpnSu4=", "iv": "1234567890123456"}
```

```text
# The `key` is typically a raw set of bytes matching the expected length for the algorithm so it is common to base64
# encode it for injection and decode in VRL
key = decode_base64!(get_env_var!("KEY")) # with $KEY set to "c2VjcmV0X19faHVudGVyMg=="


# encrypted message was stored as base64 encoded data
.message = decrypt!(decode_base64!(.encrypted_message), "AES-128-CBC-PKCS7", key, iv: .iv)

# delete originals
del(.iv)
del(.encrypted_message)
```

The result will be:

```json
{ "message": "super secret message" }
```

## Let us know what you think!

We hope users will find these new encryption functions useful for dealing with sensitive data in Vector! If you have any
feedback for us, whether it's related to the new disk buffers or anything else, let us know on [Discord] or on
[Twitter].

[AES]: https://en.wikipedia.org/wiki/Advanced_Encryption_Standard
[encrypt]: /docs/reference/vrl/functions/#encrypt
[decrypt]: /docs/reference/vrl/functions/#decrypt
[random_bytes]: /docs/reference/vrl/functions/#random_bytes
[Discord]: https://discord.gg/n3CuBAwNCn
[Twitter]: https://twitter.com/vectordotdev
