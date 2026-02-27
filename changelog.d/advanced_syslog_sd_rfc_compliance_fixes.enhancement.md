Upgrades the syslog encoding transform with three major improvements:

Structured Data Enhancements (RFC 5424):

- Supports scalars
- Handles nested objects (flattened with dot notation)
- Serializes arrays as JSON strings, e.g., `tags="[\"tag1\",\"tag2\",\"tag3\"]"` (RFC 5424 spec doesn't define how to handle arrays in structured data)
- Validates SD-ID and PARAM-NAME fields per RFC 5424
- Sanitizes invalid characters to underscores

UTF-8 Safety Fix:

- Fixes panics from byte-based truncation on multibyte characters
- Implements character-based truncation for all fields
- Prevents crashes with emojis, Cyrillic text, etc.

RFC 3164 Compliance Improvements:

- Bug fix: Structured data is now properly ignored (previously incorrectly prepended)
- TAG field sanitized to ASCII printable characters (33-126)
- Adds debug logging when structured data is ignored

authors: vparfonov
