Templates whose literal prefix started with `http://` or `https://` were incorrectly given
URI-specific confinement checks, even when the field was not a URI field (e.g. an
object-store key prefix). Confinement is now selected by the field's type rather than by
inspecting the template content, so such templates use prefix confinement instead.

authors: thomasqueirozb
