Documented that environment variable interpolation substitutes values verbatim before config parsing. Values containing structural characters such as `"`, `{`, `}`, `[`, or `]` may affect the parsed config structure. Newline characters remain the only rejected values.

authors: pront
