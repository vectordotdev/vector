Environment variable interpolation in configuration files now rejects values containing newline characters. This prevents configuration
injection attacks where environment variables could inject malicious multi-line configurations. If you need to inject multi-line
configuration blocks, use a config pre-processing tool like `envsubst` instead.

authors: pront
