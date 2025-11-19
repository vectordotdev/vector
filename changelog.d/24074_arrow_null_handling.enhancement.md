The Arrow encoder now supports configurable null handling through the `null_values`
option. This controls whether nullable fields should be explicitly marked 
as nullable in the Arrow schema, enabling better compatibility with
downstream systems that have specific requirements for null handling.

authors: benjamin-awd
