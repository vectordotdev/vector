Prevent configuration validation from panicking on macOS when a restricted environment has no
available Keychain. Vector avoids loading platform root certificates when TLS certificate
verification is disabled, unless a client identity requires native certificate-chain support.

authors: kurochan
