#!/usr/bin/env ruby
# frozen_string_literal: true

begin
  require 'json'
  require 'tempfile'
rescue LoadError => e
  puts "Load error: #{e.message}"
  exit 1
end

DEBUG_LEVEL = 1
INFO_LEVEL = 2
ERROR_LEVEL = 3

LEVEL_MAPPINGS = {
  'debug' => { 'numeric' => DEBUG_LEVEL, 'colored' => "\033[34mDEBUG\033[0m" },
  'info' => { 'numeric' => INFO_LEVEL, 'colored' => "\033[32mINFO\033[0m" },
  'error' => { 'numeric' => ERROR_LEVEL, 'colored' => "\033[31mERROR\033[0m" },
}

def numerical_level(level_str)
  LEVEL_MAPPINGS.dig(level_str.downcase, 'numeric') if !level_str.nil?
end

def colored_level(level_str)
  LEVEL_MAPPINGS.dig(level_str.downcase, 'colored') if !level_str.nil?
end

class Logger
  def initialize
    @level = numerical_level(ENV['LOG_LEVEL'] || '') || INFO_LEVEL
    @is_tty = STDOUT.isatty
  end

  def formatted_level(level)
    if @is_tty
      colored_level(level)
    else
      level.upcase
    end
  end

  def log(level, msg)
    numeric_level = numerical_level(level)
    if numeric_level >= @level
      formatted_level = self.formatted_level(level)
      dt = Time.now.strftime('%Y-%m-%dT%H:%M:%S')
      puts "[#{dt}] #{formatted_level} #{msg}"
    end
  end

  def debug(msg)
    self.log('debug', msg)
  end

  def info(msg)
    self.log('info', msg)
  end

  def error(msg)
    self.log('error', msg)
  end
end

@logger = Logger.new

@integer_schema_types = %w[uint int]
@number_schema_types = %w[float]
@numeric_schema_types = @integer_schema_types + @number_schema_types

# Cross-platform friendly method of finding if command exists on the current path.
#
# If the command is found, the full path to it is returned. Otherwise, `nil` is returned.
def find_command_on_path(command)
  exts = ENV['PATHEXT'] ? ENV['PATHEXT'].split(';') : ['']
  ENV['PATH'].split(File::PATH_SEPARATOR).each do |path|
    exts.each do |ext|
      maybe_command_path = File.join(path, "#{command}#{ext}")
      return maybe_command_path if File.executable?(maybe_command_path) && !File.directory?(maybe_command_path)
    end
  end
  nil
end

@cue_binary_path = find_command_on_path('cue')

# Helpers for caching resolved/expanded schemas and detecting schema resolution cycles.
@resolved_schema_cache = {}
@expanded_schema_cache = {}

# Gets the schema of the given `name` from the resolved schema cache, if it exists.
def get_cached_resolved_schema(schema_name)
  @resolved_schema_cache[schema_name]
end

# Gets the schema of the given `name` from the expanded schema cache, if it exists.
def get_cached_expanded_schema(schema_name)
  @expanded_schema_cache[schema_name]
end

# Generic helpers for making working with Ruby a bit easier.
def to_pretty_json(value)
  if value.is_a?(Hash)
    JSON.pretty_generate(Hash[*value.sort.flatten])
  else
    JSON.pretty_generate(value)
  end
end

def deep_copy(obj)
  Marshal.load(Marshal.dump(obj))
end

def mergeable?(value)
  value.is_a?(Hash) || value.is_a?(Array)
end

def _nested_merge_impl(base, override, merger)
  # Handle some basic cases first.
  if base.nil?
    return override
  elsif override.nil?
    return base
  elsif !mergeable?(base) && !mergeable?(override)
    return override
  end

  deep_copy(base).merge(override.to_h, &merger)
end

def nested_merge(base, override)
  merger = proc { |_, v1, v2|
    if v1.is_a?(Hash) && v2.is_a?(Hash)
      v1.merge(v2, &merger)
    elsif v1.is_a?(Array) && v2.is_a?(Array)
      v1 | v2
    else
      [:undefined, nil, :nil].include?(v2) ? v1 : v2
    end
  }
  _nested_merge_impl(base, override, merger)
end

def schema_aware_nested_merge(base, override)
  merger = proc { |key, v1, v2|
    if v1.is_a?(Hash) && v2.is_a?(Hash)
      # Special behavior for merging const schemas together so they can be properly enum-ified.
      if key == 'const' && v1.has_key?('value') && v2.has_key?('value')
        [v1].flatten | [v2].flatten
      else
        v1.merge(v2, &merger)
      end
    elsif v1.is_a?(Array) && v2.is_a?(Array)
      v1 | v2
    else
      [:undefined, nil, :nil].include?(v2) ? v1 : v2
    end
  }
  _nested_merge_impl(base, override, merger)
end

def sort_hash_nested(input)
  input.keys.sort.each_with_object({}) do |key, acc|
    acc[key] = if input[key].is_a?(Hash)
      sort_hash_nested(input[key])
    else
      input[key]
    end
  end
end

def write_to_temp_file(prefix, data)
  file = Tempfile.new(prefix)
  file.write(data)
  file.close

  file.path
end

# Gets the JSON Schema-compatible type name for the given Ruby Value.
def json_type_str(value)
  if value.is_a?(String)
    'string'
  elsif value.is_a?(Integer)
    'integer'
  elsif value.is_a?(Float)
    'number'
  elsif [true, false].include?(value)
    'boolean'
  elsif value.is_a?(Array)
    'array'
  elsif value.is_a?(Hash)
    'object'
  else
    'null'
  end
end

# Gets the docs-compatible type name for the given Ruby value.
#
# This is slightly different from the JSON Schema types, and is mostly an artifact of the original
# documentation design, and not representative of anything fundamental.
def docs_type_str(value)
  type_str = json_type_str(value)
  type_str = 'bool' if type_str == 'boolean'
  type_str
end

# Gets the type of the resolved schema.
#
# If the resolved schema has more than one type defined, `nil` is returned.
def resolved_schema_type?(resolved_schema)
  if resolved_schema['type'].length == 1
    resolved_schema['type'].keys.first
  end
end

# Gets the numeric type of the resolved schema.
#
# If the resolved schema has more than one type defined, or the type is not a numeric type, `nil` is
# returned.
def numeric_schema_type(resolved_schema)
  schema_type = resolved_schema_type?(resolved_schema)
  schema_type if @numeric_schema_types.include?(schema_type)
end

# Gets the docs type for the given value's type.
#
# When dealing with a source schema, and trying to get the "docs"-compatible schema type, we need to
# cross-reference the original schema with the type of the value we have on the Ruby side. While
# some types like a string will always have a type of "string", numbers represent an area where we
# need to do some digging.
#
# For example, we encode the specific class of number on the source schema -- unsigned, signed, or
# floating-point -- and we can discern nearly as much from the Ruby value itself (integer vs float),
# but we need to be able to discern the precise type i.e. the unsigned vs signed vs floating-point
# bit.
#
# This function cross-references the Ruby value given with the source schema it is associated with
# and returns the appropriate "docs" schema type for that value. If the value is not recognized, or
# if the source schema does not match the given value, `nil` is returned.
#
# Otherwise, the most precise "docs" schema type for the given value is returned.
def get_docs_type_for_value(schema, value)
  # If there's no schema to check against, or there is but it has no type field, that means we're
  # dealing with something like a complex, overlapping `oneOf` subschema, where we couldn't
  # declaratively figure out the right field to dig into if we were discerning an integer/number
  # value, and so on.
  #
  # We just use the detected value type in that case.
  schema_instance_type = get_json_schema_instance_type(schema) unless schema.nil?
  if schema.nil? || schema_instance_type.nil?
    return docs_type_str(value)
  end

  # If the schema defines a type, see if it matches the value type. If it doesn't, that's a bad sign
  # and we abort. Otherwise, we fallthrough below to make sure we're handling special cases i.e.
  # numeric types.
  value_type = json_type_str(value)
  if value_type != schema_instance_type
    @logger.error "Schema instance type and value type are a mismatch, which should not happen."
    @logger.error "Schema instance type: #{schema_instance_type}"
    @logger.error "Value: #{value} (type: #{value_type})"
    exit 1
  end

  # For any numeric type, extract the value of `docs::numeric_type`, which must always be present in
  # the schema for numeric fields. If the schema is `nil`, though, then it means we're dealing with
  # a complex schema (like an overlapping `oneOf` subschema, etc) and we just fallback to the
  # detected type.
  if ['number', 'integer'].include?(value_type)
    numeric_type = get_schema_metadata(schema, 'docs::numeric_type')
    if numeric_type.nil?
      @logger.error "All fields with numeric types should have 'docs::numeric_type' metadata included." +
        "e.g. #[configurable(metadata(docs::numeric_type = \"bytes\"))]"
      @logger.error "Value: #{value} (type: #{value_type})"
      exit 1
    end

    return numeric_type
  end

  # TODO: The documentation should really just use `boolean` to match JSON Schema, which would let
  # us get rid of this weird `json_type_str`/`docs_type_str` dichotomy.
  docs_type_str(value)
end

# Gets the schema type field for the given value's type.
#
# Essentially, as we resolve a schema we end up with a hash that looks like this:
#
# { "type" => { "string" => { ... } }
#
# When we want to do something like specify a default value, or give example values, we need to set
# them on the hash that represents the actual property value type. If a schema resolves as a string
# schema, we can trivially take that default value, calculate its type, and know that we need to set
# further data under the `string` key in the above example.
#
# This gets trickier for numeric types, however, as we encode them more specifically -- unsigned
# integer, floating-point number, etc -- in the resolved schema... but can only determine (on the
# Ruby side) if a value is the `number` type. To handle this, for any value of the type `number`, we
# iteratively try and find a matching type definition in the resolved schema for any of the possible
# numeric types.
def get_json_schema_type_field_for_value(source_schema, resolved_schema, value)
  value_type = get_docs_type_for_value(source_schema, value)
  resolved_schema.dig('type', value_type)
end

# Tries to find the schema for an object property nested in the given schema.
#
# This function will search through either the properties of the schema itself, if it is an object
# schema, or the properties of any object subschema that is present in `oneOf`/`allOf`.
#
# If no property is found, `nil` is returned.
def find_nested_object_property_schema(schema, property_name)
  # See if we're checking an object schema directly.
  if !schema['properties'].nil?
    return schema['properties'][property_name]
  end

  # The schema isn't an object schema, so check to see if it's a `oneOf`/`allOf`, and if so,
  # recursively visit each of those subschemas, looking for object schemas along the way that we can
  # check for the given property within.
  matching_property_schemas = []
  unvisited_subschemas = schema['oneOf'].dup || schema['anyOf'].dup|| schema['allOf'].dup || []
  while !unvisited_subschemas.empty? do
    unvisited_subschema = unvisited_subschemas.pop

    # If the subschema has object properties, it won't be `oneOf`/`allOf`, so just try and grab the
    # property if it exists, and move on.
    if !unvisited_subschema['properties'].nil?
      subschema_property = unvisited_subschema.dig('properties', property_name)
      matching_property_schemas.push(subschema_property) unless subschema_property.nil?
      next
    end

    # If the subschema had no object properties, see if it's an `oneOf`/`allOf` subschema, and if
    # so, collect any of _those_ subschemas and add them to our list of subschemas to visit.
    maybe_unvisited_subschemas = unvisited_subschema['oneOf'].dup || unvisited_subschema['anyOf'].dup || unvisited_subschema['allOf'].dup || []
    unvisited_subschemas.concat(maybe_unvisited_subschemas) unless maybe_unvisited_subschemas.nil?
  end

  # Compare all matching property schemas to each other -- in their reduced form -- to see if they're
  # identical. If they're not, or there were no matches, return `nil`.
  #
  # Otherwise, return the first matching property schema.
  reduced_matching_property_schemas = matching_property_schemas.map { |schema| get_reduced_schema(schema) }
  matching_property_schemas[0] unless reduced_matching_property_schemas.uniq.count != 1
end

def get_schema_metadata(schema, key)
  schema.dig('_metadata', key)
end

def get_schema_ref(schema)
  schema['$ref']
end

# Gets the schema type for the given schema.
def get_json_schema_type(schema)
  if schema.key?('allOf')
    'all-of'
  elsif schema.key?('oneOf')
    'one-of'
  elsif schema.key?('anyOf')
    'any-of'
  elsif schema.key?('type')
    get_json_schema_instance_type(schema)
  elsif schema.key?('const')
    'const'
  elsif schema.key?('enum')
    'enum'
  end
end

def get_json_schema_instance_type(schema)
  maybe_type = schema['type']

  # We don't deal with null instance types at all in the documentation generation phase.
  if maybe_type == 'null'
    return nil
  end

  # If the schema specifies multiple instance types, see if `null` is one of them, and if so,
  # remove it. After that, if only one value is left, return that value directly rather than
  # wrapped in an array.
  #
  # Otherwise, return the original array.
  if maybe_type.is_a?(Array)
    filtered = maybe_type.reject { |instance_type| instance_type == "null" }
    if filtered.length == 1
      return filtered[0]
    end
  end

  maybe_type
end

# Fixes grouped enums by adjusting the schema type where necessary.
#
# For "grouped enums", these represent the sum of all possible enum values in an `enum` schema being
# grouped by their JSON type. For example, a set of enums such as `[0, 1, 2, true, false]` would be
# grouped as:
#
# { "bool": [true, false], "number": [0, 1, 2] }
#
# This is technically correct, but in the documentation output, that `number` key needs to be `uint`
# or `int` or what have you. Since `enum` schemas don't carry the "numeric type" information, we try
# and figure that out here.
#
# If we find a `number` group, we check all of its values to see if they fit neatly within the
# bounds of any of the possible numeric types `int`, `uint`, or `float`. We try and coalesce
# towards `uint` as it's by far the most common numeric type in Vector configuration, but after
# that, we aim for `int`, unless the values are too large, in which case we'll shift up to `float`.
def fix_grouped_enums_if_numeric!(grouped_enums)
  ['integer', 'number'].each { |type_name|
    number_group = grouped_enums.delete(type_name)
    if !number_group.nil?
      is_integer = number_group.all? { |n| n.is_a?(Integer) }
      within_uint = number_group.all? { |n| n >= 0 && n <= 2 ** 64 }
      within_int = number_group.all? { |n| n >= -(2 ** 63) && n <= (2 ** 63) - 1 }

      # If the values themselves are not all integers, or they are but not all of them can fit within
      # a normal 64-bit signed/unsigned integer, then we use `float` as it's the only other type that
      # could reasonably satisfy the constraints.
      numeric_type = if !is_integer || (!within_int && !within_uint)
        'float'
      else
        if within_uint
          'uint'
        elsif within_int
          'int'
        else
          # This should never actually happen, _but_, technically Ruby integers could be a "BigNum"
          # aka arbitrary-precision integer, so this protects us if somehow we get a value that is an
          # integer but doesn't actually fit neatly into 64 bits.
          'float'
        end
      end

      grouped_enums[numeric_type] = number_group
    end
  }
end

# Gets a schema definition from the root schema, by name.
def get_schema_by_name(root_schema, schema_name)
  schema_name = schema_name.gsub(%r{#/definitions/}, '')
  schema_def = root_schema.dig('definitions', schema_name)
  if schema_def.nil?
    @logger.error "Could not find schema definition '#{schema_name}' in given schema."
    exit 1
  end

  schema_def
end

# Gets the dereferenced version of this schema.
#
# If the schema has no schema reference, `nil` is returned.
def dereferenced_schema(schema)
  schema_name = get_schema_ref(schema)
  if !schema_ref.nil?
    get_schema_by_name(root_schema, schema_name)
  end
end

# Applies various fields to an object property.
#
# This includes items such as any default value that is present, or whether or not the property is
# required.
def apply_object_property_fields!(parent_schema, property_schema, property_name, property)
  @logger.debug "Applying object property fields for '#{property_name}'..."

  required_properties = parent_schema['required'] || []
  has_self_default_value = !property_schema['default'].nil?
  has_parent_default_value = !parent_schema.dig('default', property_name).nil?
  has_default_value = has_self_default_value || has_parent_default_value
  is_required = required_properties.include?(property_name)

  if has_self_default_value
    @logger.debug "Property has self-defined default value: #{property_schema['default']}"
  end

  if has_parent_default_value
    @logger.debug "Property has parent-defined default value: #{parent_schema.dig('default', property_name)}"
  end

  if is_required
    @logger.debug "Property is marked as required."
  end

  # Set whether or not this property is required.
  property['required'] = required_properties.include?(property_name) && !has_default_value
end

# Expands any schema references in the given schema.
#
# If the schema contains a top-level schema reference, or if any of the parts of its schema contain
# schema references (array items schema, any subschemas in `oneOf`/`allOf`, etc), then those
# references are expanded. Expansion happens recursively until all schema references
#
# For any overlapping fields in the given schema and the referenced schema, the fields from the
# given schema will win.
def expand_schema_references(root_schema, unexpanded_schema)
  schema = deep_copy(unexpanded_schema)

  # Grab the existing title/description from our unexpanded schema, and reset them after
  # merging. This avoids us adding a title where there was only a description, and so on, since
  # we have special handling rules around titles vs descriptions.
  #
  # TODO: If we ever just get rid of the title/description dichotomy, we could clean up this
  # logic.
  original_title = unexpanded_schema['title']
  original_description = unexpanded_schema['description']

  loop do
    expanded = false

    # If the schema has a top level reference, we expand it.
    schema_ref = schema['$ref']
    if !schema_ref.nil?
      expanded_schema_ref = get_cached_expanded_schema(schema_ref)
      if expanded_schema_ref.nil?
        @logger.debug "Expanding top-level schema ref of '#{schema_ref}'..."

        unexpanded_schema_ref = get_schema_by_name(root_schema, schema_ref)
        expanded_schema_ref = expand_schema_references(root_schema, unexpanded_schema_ref)

        @expanded_schema_cache[schema_ref] = expanded_schema_ref
      end

      schema.delete('$ref')
      schema = nested_merge(expanded_schema_ref, schema)

      expanded = true
    end

    # If the schema is an array type and has a reference for its items, we expand that.
    items_ref = schema.dig('items', '$ref')
    if !items_ref.nil?
      expanded_items_schema_ref = expand_schema_references(root_schema, schema['items'])

      schema['items'].delete('$ref')
      schema['items'] = nested_merge(expanded_items_schema_ref, schema['items'])

      expanded = true
    end

    # If the schema has any object properties, we expand those.
    if !schema['properties'].nil?
      schema['properties'] = schema['properties'].transform_values { |property_schema|
        new_property_schema = expand_schema_references(root_schema, property_schema)
        if new_property_schema != property_schema
          expanded = true
        end

        new_property_schema
      }
    end

    # If the schema has any `allOf`/`oneOf` subschemas, we expand those, too.
    if !schema['allOf'].nil?
      schema['allOf'] = schema['allOf'].map { |subschema|
        new_subschema = expand_schema_references(root_schema, subschema)
        if new_subschema != subschema
          expanded = true
        end

        new_subschema
      }
    end

    if !schema['oneOf'].nil?
      schema['oneOf'] = schema['oneOf'].map { |subschema|
        new_subschema = expand_schema_references(root_schema, subschema)
        if new_subschema != subschema
          expanded = true
        end

        new_subschema
      }
    end

    if !schema['anyOf'].nil?
      schema['anyOf'] = schema['anyOf'].map { |subschema|
        new_subschema = expand_schema_references(root_schema, subschema)
        if new_subschema != subschema
          expanded = true
        end

        new_subschema
      }
    end

    if !expanded
      break
    end
  end

  # If the original schema had either a title or description, we forcefully reset both of them back
  # to their original state, either in terms of their value or them not existing as fields.
  #
  # If neither were present, we allow the merged in title/description, if any, to persist, as this
  # maintains the "#[configurable(derived)]" behavior of titles/descriptions for struct fields.
  if !original_title.nil? || !original_description.nil?
    if !original_title.nil?
      schema['title'] = original_title
    else
      schema.delete('title')
    end

    if
      schema['description'] = original_description
    else
      schema.delete('description')
    end
  end

  schema
end

# Gets a reduced version of a schema.
#
# The reduced version strips out extraneous fields from the given schema, such that a value should
# be returned that is suitable for comparison with other schemas, to determine if the schemas --
# specifically the values that are allowed/valid -- are the same, while ignoring things like titles
# and descriptions.
def get_reduced_schema(schema)
  schema = deep_copy(schema)

  allowed_properties = ['type', 'const', 'enum', 'allOf', 'oneOf', '$ref', 'items', 'properties']
  schema.delete_if { |key, _value| !allowed_properties.include?(key) }

  if schema.key?('items')
    schema['items'] = get_reduced_schema(schema['items'])
  end

  if schema.key?('properties')
    schema['properties'] = schema['properties'].transform_values { |property_schema| get_reduced_schema(property_schema) }
  end

  if schema.key?('allOf')
    schema['allOf'] = schema['allOf'].map { |subschema| get_reduced_schema(subschema) }
  end

  if schema.key?('oneOf')
    schema['oneOf'] = schema['oneOf'].map { |subschema| get_reduced_schema(subschema) }
  end

  schema
end

# Gets a reduced version of a resolved schema.
#
# This is similar in purpose to `get_reduced_schema` but only cares about fields relevant to a
# resolved schema.
def get_reduced_resolved_schema(schema)
  schema = deep_copy(schema)

  allowed_types = ['condition', 'object', 'array', 'enum', 'const', 'string', 'bool', 'float', 'int', 'uint']
  allowed_fields = []

  # Clear out anything not related to the type definitions first.
  schema.delete_if { |key, _value| key != 'type' }
  type_defs = schema['type']
  if !type_defs.nil?
    type_defs.delete_if { |key, _value| !allowed_types.include?(key) }
    schema['type'].each { |type_name, type_def|
      case type_name
      when "object"
        type_def.delete_if { |key, _value| key != 'options' }
        type_def['options'].transform_values! { |property|
          get_reduced_resolved_schema(property)
        }
      when "array"
        type_def.delete_if { |key, _value| key != 'items' }
        type_def['items'] = get_reduced_resolved_schema(type_def['items'])
      else
        type_def.delete_if { |key, _value| !allowed_types.include?(key) }
      end
    }
  end

  schema
end

# Fully resolves a schema definition, if it exists.
#
# This looks up a schema definition by the given `name` within `root_schema` and resolves it.
# If no schema definition exists for the given name, `nil` is returned. Otherwise, the schema
# definition is preprocessed (collapsing schema references, etc), and then resolved. Both the
# "source" schema (preprocessed value) and the resolved schema are returned.
#
# Resolved schemas are cached.
#
# See `resolve_schema` for more details.
def resolve_schema_by_name(root_schema, schema_name)
  # If it's already cached, use that.
  resolved = get_cached_resolved_schema(schema_name)
  return deep_copy(resolved) unless resolved.nil?

  # It wasn't already cached, so we actually have to resolve it.
  schema = get_schema_by_name(root_schema, schema_name)
  resolved = resolve_schema(root_schema, schema)
  @resolved_schema_cache[schema_name] = resolved
  deep_copy(resolved)
end

# Fully resolves the schema.
#
# This recursively resolves schema references, as well as flattening them into a single object, and
# transforming certain usages -- composite/enum (`allOf`, `oneOf`), etc -- into more human-readable
# forms.
def resolve_schema(root_schema, schema)
  # If the schema we've been given if a schema reference, we expand that first. This happens
  # recursively, such that the resulting expanded schema has no schema references left. We need this
  # because in further steps, we need the access to the full input schema that was used to generate
  # the resolved schema.
  schema = expand_schema_references(root_schema, schema)

  # Skip any schema that is marked as hidden.
  #
  # While this is already sort of obvious, one non-obvious usage is for certain types that we
  # manually merge after this script runs, such as the high-level "outer" (`SinkOuter`, etc) types.
  # Those types include a field that uses the Big Enum -- an enum with all possible components of
  # that type -- which, if we resolved it here, would spit out a ton of nonsense.
  #
  # We mark that field hidden so that it's excluded when we resolve the schema for `SinkOuter`, etc,
  # and then we individually resolve the component schemas, and merge the two (outer + component
  # itself) schemas back together.
  if get_schema_metadata(schema, 'docs::hidden')
    @logger.debug 'Instructed to skip resolution for the given schema.'
    return
  end

  # Handle schemas that have type overrides.
  #
  # In order to better represent specific field types in the documentation, we may opt to use a
  # special type definition name, separate from the classic types like "bool" or "string" or
  # "object", and so on, in order to let the documentation generation process inject more
  # full-fledged output than we can curry from the Rust side, across the configuration schema.
  #
  # We intentially set no actual definition for these types, relying on the documentation generation
  # process to provide the actual details. We only need to specify the custom type name.
  #
  # To handle u8 types as ascii characters and not there uint representation between 0 and 255 we
  # added a special handling of these exact values. This means
  # `#[configurable(metadata(docs::type_override = "ascii_char"))]` should only be used consciously
  # for rust u8 type. See lib/codecs/src/encoding/format/csv.rs for an example and
  # https://github.com/vectordotdev/vector/pull/20498
  type_override = get_schema_metadata(schema, 'docs::type_override')
  if !type_override.nil?
    if type_override == 'ascii_char'
      if !schema['default'].nil?
        resolved = { 'type' => { type_override.to_s => { 'default' => schema['default'].chr } } }
      else
        resolved = { 'type' => { type_override.to_s => { } } }
      end
    else
      resolved = { 'type' => { type_override.to_s => {} } }
    end
    description = get_rendered_description_from_schema(schema)
    resolved['description'] = description unless description.empty?
    return resolved
  end

  # Now that the schema is fully expanded and it didn't need special handling, we'll go ahead and
  # fully resolve it.
  resolved = resolve_bare_schema(root_schema, schema)
  if resolved.nil?
    return
  end

  # If this is an array schema, remove the description from the schema used for the items, as we
  # want the description for the overall property, using this array schema, to describe everything.
  items_schema = resolved.dig('type', 'array', 'items')
  if !items_schema.nil?
    items_schema.delete('description')
  end

  # Apply any necessary defaults, descriptions, etc, to the resolved schema. This must happen here
  # because there could be callsite-specific overrides to defaults, descriptions, etc, for a given
  # schema definition that have to be layered.
  apply_schema_default_value!(schema, resolved)
  apply_schema_metadata!(schema, resolved)

  description = get_rendered_description_from_schema(schema)
  resolved['description'] = description unless description.empty?

  ## Resolve the deprecated flag. An optional deprecated message can also be set in the metadata.
  if schema.fetch('deprecated', false)
    resolved['deprecated'] = true
    message = get_schema_metadata(schema, 'deprecated_message')
    if message
      resolved['deprecated_message'] = message
    end
  end

  # required for global option configuration
  is_common_field = get_schema_metadata(schema, 'docs::common')
  if !is_common_field.nil?
    resolved['common'] = is_common_field
  end

  is_required_field = get_schema_metadata(schema, 'docs::required')
  if !is_required_field.nil?
    resolved['required'] = is_required_field
  end

  # Reconcile the resolve schema, which essentially gives us a chance to, once the schema is
  # entirely resolved, check it for logical inconsistencies, fix up anything that we reasonably can,
  # and so on.
  reconcile_resolved_schema!(resolved)

  resolved
end

# Fully resolves a bare schema.
#
# A bare schema is one that has no references to another schema, etc.
def resolve_bare_schema(root_schema, schema)
  resolved = case get_json_schema_type(schema)
    when 'all-of'
      @logger.debug 'Resolving composite schema.'

      # Composite schemas are indeed the sum of all of their parts, so resolve each subschema and
      # merge their resolved state together.
      reduced = schema['allOf'].filter_map { |subschema| resolve_schema(root_schema, subschema) }
                              .reduce { |acc, item| nested_merge(acc, item) }
      reduced['type']
    when 'one-of', 'any-of'
      @logger.debug 'Resolving enum schema.'

      # We completely defer resolution of enum schemas to `resolve_enum_schema` because there's a
      # lot of tricks and matching we need to do to suss out patterns that can be represented in more
      # condensed resolved forms.
      wrapped = resolve_enum_schema(root_schema, schema)

      # `resolve_enum_schema` always hands back the resolved schema under the key `_resolved`, so
      # that we can add extra details about the resolved schema (anything that will help us render
      # it better in the docs output) on the side. That's why we have to unwrap the resolved schema
      # like this.

      # TODO: Do something with the extra detail (`annotations`).

      wrapped.dig('_resolved', 'type')
    when 'array'
      @logger.debug 'Resolving array schema.'

      { 'array' => { 'items' => resolve_schema(root_schema, schema['items']) } }
    when 'object'
      @logger.debug 'Resolving object schema.'

      # TODO: Not all objects have an actual set of properties, such as anything using
      # `additionalProperties` to allow for arbitrary key/values to be set, which is why we're
      # handling the case of nothing in `properties`... but we probably want to be able to better
      # handle expressing this in the output.. or maybe it doesn't matter, dunno!
      properties = schema['properties'] || {}

      options = properties.filter_map do |property_name, property_schema|
        @logger.debug "Resolving object property '#{property_name}'..."
        resolved_property = resolve_schema(root_schema, property_schema)
        if !resolved_property.nil?
          apply_object_property_fields!(schema, property_schema, property_name, resolved_property)

          @logger.debug "Resolved object property '#{property_name}'."

          [property_name, resolved_property]
        else
          @logger.debug "Resolution failed for '#{property_name}'."

          nil
        end
      end

      # If the object schema has `additionalProperties` set, we add an additional field
      # (`*`) which uses the specified schema for that field.
      additional_properties = schema['additionalProperties']
      if !additional_properties.nil?
        @logger.debug "Handling additional properties."

        # Normally, we only get here if there's a hashmap field on a struct that can act as the
        # catch-all for additional properties. That field, by definition, will be required to have a
        # description, and maybe will have a title.
        #
        # That title/description makes sense for the field itself, but when we generate this new
        # wildcard property, we generally want to have something short and simple, in the singular
        # form. For example, if we have a field called "labels", the title/description might talk
        # about what the labels are used for, any special requirements, and so on... and then for
        # the wildcard property, we might want to have the description read as "A foobar label."
        # just to make the UI look nice.
        #
        # Rather than try and derive this from the title/description on the field, we'll require
        # such a description to be provided on the Rust side via the metadata attribute shown below.
        singular_description = get_schema_metadata(schema, 'docs::additional_props_description')
        if singular_description.nil?
          @logger.error "Missing 'docs::additional_props_description' metadata for a wildcard field.\n\n" \
          "For map fields (`HashMap<...>`, etc), a description (in the singular form) must be provided by via `#[configurable(metadata(docs::additional_props_description = \"Description of the field.\"))]`.\n\n" \
          "The description on the field, derived from the code comments, is shown specifically for `field`, while the description provided via `docs::additional_props_description` is shown for the special `field.*` entry that denotes that the field is actually a map."

          @logger.error "Relevant schema: #{JSON.pretty_generate(schema)}"
          exit 1
        end

        resolved_additional_properties = resolve_schema(root_schema, additional_properties)
        resolved_additional_properties['required'] = true
        resolved_additional_properties['description'] = singular_description
        options.push(['*', resolved_additional_properties])
      end

      { 'object' => { 'options' => options.to_h } }
    when 'string'
      @logger.debug 'Resolving string schema.'

      string_def = {}
      string_def['default'] = schema['default'] unless schema['default'].nil?

      { 'string' => string_def }
    when 'number', 'integer'
      @logger.debug 'Resolving number schema.'

      numeric_type = get_schema_metadata(schema, 'docs::numeric_type') || 'number'
      number_def = {}
      number_def['default'] = schema['default'] unless schema['default'].nil?

      @logger.debug 'Resolved number schema.'

      { numeric_type => number_def }
    when 'boolean'
      @logger.debug 'Resolving boolean schema.'

      bool_def = {}
      bool_def['default'] = schema['default'] unless schema['default'].nil?

      { 'bool' => bool_def }
    when 'const'
      @logger.debug 'Resolving const schema.'

      # For `const` schemas, just figure out the type of the constant value so we can generate the
      # resolved output.
      const_type = get_docs_type_for_value(schema, schema['const'])
      const_value = { 'value' => schema['const'] }
      const_description = get_rendered_description_from_schema(schema)
      const_value['description'] = const_description unless const_description.nil?
      { const_type => { 'const' => const_value } }
    when 'enum'
      @logger.debug 'Resolving enum const schema.'

      # Similarly to `const` schemas, `enum` schemas are merely multiple possible constant values. Given
      # that JSON Schema does allow for the constant values to differ in type, we group them all by
      # type to get the resolved output.
      enum_values = schema['enum']
      grouped = enum_values.group_by { |value| docs_type_str(value) }
      fix_grouped_enums_if_numeric!(grouped)
      grouped.transform_values! { |values| { 'enum' => values } }
      grouped
    else
      @logger.error "Failed to resolve the schema. Schema: #{schema}"
      exit 1
    end

  { 'type' => resolved }
end

def resolve_enum_schema(root_schema, schema)
  # Figure out if this is a one-of or any-of enum schema. Both at the same time is never correct.
  subschemas = if schema.key?('oneOf')
    schema['oneOf']
  elsif schema.key?('anyOf')
    schema['anyOf']
  else
    @logger.error "Enum schema had both `oneOf` and `anyOf` specified. Schema: #{schema}"
    exit 1
  end

  # Filter out all subschemas which are purely null schemas used for indicating optionality, as well
  # as any subschemas that are marked as being hidden.
  is_optional = get_schema_metadata(schema, 'docs::optional')
  subschemas = subschemas
    .reject { |subschema| subschema['type'] == 'null' }
    .reject { |subschema| get_schema_metadata(subschema, 'docs::hidden') }
  subschema_count = subschemas.count

  # If we only have one subschema after filtering, check to see if it's an `allOf` or `oneOf` schema
  # and `is_optional` is true.
  #
  # If it's an `allOf` subschema, then that means we originally had an `allOf` schema that we had to
  # make optional, thus converting it to a `oneOf` with subschemas in the shape of `[null, allOf]`.
  # In this case, we'll just remove the `oneOf` and move the `allOf` subschema up, as if it this
  # schema was a `allOf` one all along.
  #
  # If so, we unwrap it such that we end up with a copy of `schema` that looks like it was an
  # `allOf` schema all along. We do this to properly resolve `allOf` schemas that were wrapped as
  # `oneOf` w/ a null schema in order to establish optionality.
  if is_optional && subschema_count == 1
    if get_json_schema_type(subschemas[0]) == 'all-of'
      @logger.debug "Detected optional all-of schema, unwrapping all-of schema to resolve..."

      # Copy the current schema and drop `oneOf` and set `allOf` with the subschema, which will get us the correct
      # unwrapped structure.
      unwrapped_schema = deep_copy(schema)
      unwrapped_schema.delete('oneOf')
      unwrapped_schema['allOf'] = deep_copy(subschemas[0]['allOf'])

      return { '_resolved' => resolve_schema(root_schema, unwrapped_schema) }
    else
      # For all other subschema types, we copy the current schema, drop the `oneOf`, and merge the
      # subschema into it. This essentially unnests the schema.
      unwrapped_schema = deep_copy(schema)
      unwrapped_schema.delete('oneOf')
      unwrapped_schema = schema_aware_nested_merge(unwrapped_schema, subschemas[0])

      return { '_resolved' => resolve_schema(root_schema, unwrapped_schema) }
    end
  end

  # Collect all of the tagging mode information upfront.
  enum_tagging = get_schema_metadata(schema, 'docs::enum_tagging')
  if enum_tagging.nil?
    @logger.error 'Enum schemas should never be missing the metadata for the enum tagging mode.'
    @logger.error "Schema: #{JSON.pretty_generate(schema)}"
    @logger.error "Filter subschemas: #{JSON.pretty_generate(subschemas)}"
    exit 1
  end

  enum_tag_field = get_schema_metadata(schema, 'docs::enum_tag_field')

  # Schema pattern: X or array of X.
  #
  # We employ this pattern on the Vector side to allow specifying a single instance of X -- object,
  # string, whatever -- or as an array of X. We just need to inspect both schemas to make sure one
  # is an array of X and the other is the same as X, or vice versa.
  if subschema_count == 2
    array_idx = subschemas.index { |subschema| subschema['type'] == 'array' }
    unless array_idx.nil?
      @logger.debug "Detected likely 'X or array of X' enum schema, applying further validation..."

      single_idx = array_idx.zero? ? 1 : 0

      # We 'reduce' the subschemas, which strips out all things that aren't fundamental, which boils
      # it down to only the shape of what the schema accepts, so no title or description or default
      # values, and so on.
      single_reduced_subschema = get_reduced_schema(subschemas[single_idx])
      array_reduced_subschema = get_reduced_schema(subschemas[array_idx])

      if single_reduced_subschema == array_reduced_subschema['items']
        @logger.debug 'Reduced schemas match, fully resolving schema for X...'

        # The single subschema and the subschema for the array items are a match! We'll resolve this
        # as the schema of the "single" option, but with an annotation that it can be specified
        # multiple times.

        # Copy the subschema, and if the parent schema we're resolving has a default value with a
        # type that matches the type of the "single" subschema, add it to the copy of that schema.
        #
        # It's hard to propagate the default from the configuration schema generation side, but much
        # easier to do so here.
        single_subschema = deep_copy(subschemas[single_idx])
        if get_json_schema_type(single_subschema) == json_type_str(schema['default'])
          single_subschema['default'] = schema['default']
        end
        resolved_subschema = resolve_schema(root_schema, subschemas[single_idx])

        @logger.debug "Resolved as 'X or array of X' enum schema."

        return { '_resolved' => resolved_subschema, 'annotations' => 'single_or_array' }
      end
    end
  end

  # Schema pattern: simple internally tagged enum with named fields.
  #
  # This a common pattern where we'll typically have enum variants that have named fields, and we
  # use an internal tag (looks like a normal field next to the fields of the enum variant itself)
  # where the tag value is the variant name.
  #
  # We want to generate an object schema where we expose the unique sum of all named fields from the
  # various subschemas, and annotate each resulting unique property with the field value that makes
  # it relevant. We do require that any overlapping fields between variants be identical, however.
  #
  # For example, buffer configuration allows configuring in-memory buffers or disk buffers. When
  # using in-memory buffers, a property called `max_events` becomes relevant, so we want to be able
  # to say that the `max_events` field is `relevant_when` the value of `type` (the adjacent tag) is
  # `memory`. We do this for every property that _isn't_ the adjacent tag, but the adjacent tag
  # _does_ get included in the resulting properties.
  if enum_tagging == 'internal'
    @logger.debug "Resolving enum subschemas to detect 'object'-ness..."

    # This transformation only makes sense when all subschemas are objects, so only resolve the ones
    # that are objects, and only proceed if all of them were resolved.
    resolved_subschemas = subschemas.filter_map do |subschema|
      # TODO: the exact enum variant probably isn't an object? but probabilistic is... gotta handle that
      resolved = resolve_schema(root_schema, subschema)
      resolved if resolved_schema_type?(resolved) == 'object'
    end

    if resolved_subschemas.count == subschemas.count
      @logger.debug "Detected likely 'internally-tagged with named fields' enum schema, applying further validation..."

      unique_resolved_properties = {}
      unique_tag_values = {}

      resolved_subschemas.each do |resolved_subschema|
        @logger.debug "Resolved subschema: #{JSON.dump(resolved_subschema)}"
        resolved_subschema_properties = resolved_subschema.dig('type', 'object', 'options')

        # Extract the tag property and figure out any necessary intersections, etc.
        #
        # Technically, a `const` value in JSON Schema can be an array or object, too... but like, we
        # only ever use `const` for describing enum variants and what not, so this is my middle-ground
        # approach to also allow for other constant-y types, but not objects/arrays which would
        # just... not make sense.
        #
        # We also steal the title and/or description from the variant subschema and apply it to the
        # tag's subschema, as we don't curry the title/description for a variant itself to the
        # respective tag field used to indicate which variant is specified.
        tag_subschema = resolved_subschema_properties.delete(enum_tag_field)
        tag_subschema['title'] = resolved_subschema['title'] if !resolved_subschema['title'].nil?
        tag_subschema['description'] = resolved_subschema['description'] if !resolved_subschema['description'].nil?
        tag_value = nil

        %w[string number integer boolean].each do |allowed_type|
          maybe_tag_values = tag_subschema.dig('type', allowed_type, 'enum')
          maybe_tag_value = maybe_tag_values.keys.first unless maybe_tag_values.nil?
          unless maybe_tag_value.nil?
            tag_value = maybe_tag_value
            break
          end
        end

        @logger.debug "Tag value of #{tag_value}, with original resolved schema: #{resolved_subschema}"

        if tag_value.nil?
          @logger.error 'All enum subschemas representing an internally-tagged enum must have the tag field use a const value.'
          @logger.error "Tag subschema: #{tag_subschema}"
          exit 1
        end

        if unique_tag_values.key?(tag_value)
          @logger.error "Found duplicate tag value '#{tag_value}' when resolving enum subschemas."
          exit 1
        end

        unique_tag_values[tag_value] = tag_subschema

        # Now merge all of the properties from the given subschema, so long as the overlapping
        # properties have the same schema.
        resolved_subschema_properties.each do |property_name, property_schema|
          existing_property = unique_resolved_properties[property_name]
          resolved_property = if !existing_property.nil?
            # The property is already being tracked, so just do a check to make sure the property from our
            # current subschema matches the existing property, schema-wise, before we update it.
            reduced_existing_property = get_reduced_resolved_schema(existing_property)
            reduced_new_property = get_reduced_resolved_schema(property_schema)

            if reduced_existing_property != reduced_new_property
              @logger.error "Had overlapping property '#{property_name}' from resolved enum subschema, but schemas differed:"
              @logger.error "Existing property schema (reduced): #{to_pretty_json(reduced_existing_property)}"
              @logger.error "New property schema (reduced): #{to_pretty_json(reduced_new_property)}"
              exit 1
            end

            @logger.debug "Adding relevant tag to existing resolved property schema for '#{property_name}'."

            # The schemas match, so just update the list of "relevant when" values.
            existing_property['relevant_when'].push(tag_value)
            existing_property
          else
            @logger.debug "First time seeing resolved property schema for '#{property_name}'."

            # First time seeing this particular property.
            property_schema['relevant_when'] = [tag_value]
            property_schema
          end

          unique_resolved_properties[property_name] = resolved_property
          @logger.debug "Set unique resolved property '#{property_name}' to resolved schema: #{resolved_property}"
        end
      end

      # Now that we've gone through all of the non-tag field, possibly overlapped properties, go
      # through and modify the properties so that we only keep the "relevant when" values if the
      # list of those values does not match the full set of unique tag values. We don't want to show
      # "relevant when" for fields that all variants share, basically.
      unique_tags = unique_tag_values.keys

      unique_resolved_properties.transform_values! do |value|
        # We check if a given property is relevant to all tag values by getting an intersection
        # between `relevant_when` and the list of unique tag values, as well as asserting that the
        # list lengths are identical.
        relevant_when = value['relevant_when']
        if relevant_when.length == unique_tags.length && relevant_when & unique_tags == unique_tags
          value.delete('relevant_when')
        end

        # Add enough information from consumers to figure out _which_ field needs to have the given
        # "relevant when" value.
        if value.key?('relevant_when')
          value['relevant_when'] = value['relevant_when'].map do |value|
            "#{enum_tag_field} = #{JSON.dump(value)}"
          end.to_a.join(' or ')
        end

        value
      end

      # Now we build our property for the tag field itself, and add that in before returning all of
      # the unique resolved properties.
      enum_values = unique_tag_values.transform_values do |tag_schema|
        @logger.debug "Tag schema: #{tag_schema}"
        get_rendered_description_from_schema(tag_schema)
      end

      @logger.debug "Resolved enum values for tag property: #{enum_values}"
      resolved_tag_property = {
        'required' => true,
        'type' => {
          'string' => {
            'enum' => enum_values
          }
        }
      }

      tag_description = get_schema_metadata(schema, 'docs::enum_tag_description')
      if tag_description.nil?
        @logger.error "A unique tag description must be specified for enums which are internally tagged (i.e. `#[serde(tag = \"...\")/`). This can be specified via `#[configurable(metadata(docs::enum_tag_description = \"...\"))]`."
        @logger.error "Schema being generated: #{JSON.pretty_generate(schema)}"
        exit 1
      end
      resolved_tag_property['description'] = tag_description
      unique_resolved_properties[enum_tag_field] = resolved_tag_property

      @logger.debug "Resolved as 'internally-tagged with named fields' enum schema."
      @logger.debug "Resolved properties for ITNF enum schema: #{unique_resolved_properties}"

      return { '_resolved' => { 'type' => { 'object' => { 'options' => unique_resolved_properties } } } }
    end
  end

  # Schema pattern: simple externally tagged enum with only unit variants.
  #
  # This a common pattern where basic enums that only have unit variants -- i.e. `enum { A, B, C }`
  # -- end up being represented by a bunch of subschemas that are purely `const` values.
  if enum_tagging == 'external'
    tag_values = {}

    subschemas.each do |subschema|
      # For each subschema, try and grab the value of the `const` property and use it as the key for
      # storing this subschema.
      #
      # We take advantage of missing key index gets returning `nil` by checking below to make sure
      # none of the keys are nil. If any of them _are_ nill, then we know not all variants had a
      # `const` schema.
      tag_values[subschema['const']] = subschema
    end

    if tag_values.keys.all? { |tag| !tag.nil? && tag.is_a?(String) }
      @logger.debug "Resolved as 'externally-tagged with only unit variants' enum schema."

      return { '_resolved' => { 'type' => { 'string' => {
        'enum' => tag_values.transform_values { |tag_schema| get_rendered_description_from_schema(tag_schema) }
      } } } }
    end
  end

  # Schema pattern: untagged enum with narrowing constant variants and catch-all free-form variant.
  #
  # This a common pattern where an enum might represent a particular single value type, say a
  # string, and it contains multiple variants where one is a "dynamic" variant and the others are
  # "fixed", such that the dynamic variant can represent all possible string values _except_ for the
  # string values defined by each fixed variant.
  #
  # An example of this is the `TimeZone` enum, where there is one variant `Local`, represented by
  # `"local"`, and the other variant `Named` can represent any other possible timezone.
  if enum_tagging == 'untagged'
    type_def_kinds = []
    fixed_subschemas = 0
    freeform_subschemas = 0

    subschemas.each do |subschema|
      @logger.debug "Untagged subschema: #{subschema}"
      schema_type = get_json_schema_type(subschema)
      case schema_type
      when nil, "all-of", "one-of"
        # We don't handle these cases.
      when "const"
        # Track the type definition kind associated with the constant value.
        type_def_kinds << docs_type_str(subschema['const'])
        fixed_subschemas = fixed_subschemas + 1
      when "enum"
        # Figure out the type definition kind for each enum value and track it.
        type_def_kinds << subschema['enum'].map { |value| docs_type_str(value) }
        fixed_subschemas = fixed_subschemas + 1
      else
        # Just a regular schema type, so track it.
        type_def_kinds << schema_type
        freeform_subschemas = freeform_subschemas + 1
      end
    end

    # If there's more than a single type definition in play, then it doesn't qualify.
    unique_type_def_kinds = type_def_kinds.flatten.uniq
    if unique_type_def_kinds.length == 1 && fixed_subschemas >= 1 && freeform_subschemas == 1
      @logger.debug "Resolved as 'untagged with narrowed free-form' enum schema."

      type_def_kind = unique_type_def_kinds[0]

      # TODO: It would be nice to forward along the fixed values so they could be enumerated in the
      # documentation, and we could have a boilerplate blurb about "these values are fixed/reserved,
      # but any other value than these can be passed", etc.

      return { '_resolved' => { 'type' => { type_def_kind => {} } }, 'annotations' => 'narrowed_free_form' }
    end
  end

  # Schema pattern: simple externally tagged enum with only non-unit variants.
  #
  # This pattern represents an enum where the default external tagging mode is used, which leads to
  # a schema for each variant that looks like it's an object with a single property -- the name of
  # which is the name of the variant itself -- and the schema of that property representing whatever
  # the fields are for the variant.
  #
  # Contrasted with an externally tagged enum with only unit variants, this type of enum is treated
  # like an object itself, where each possible variant is its own property, with whatever the
  # resulting subschema for that variant may be.
  if enum_tagging == 'external'
    # With external tagging, and non-unit variants, each variant must be represented as an object schema.
    if subschemas.all? { |subschema| get_json_schema_type(subschema) == "object" }
      # Generate our overall object schema, where each variant is a property on this schema. The
      # schema of that property should be the schema for the variant's tagging field. For example,
      # a variant called `Foo` will have an object schema with a single, required field `foo`. We
      # take the schema of that property `foo`, and set it as the schema for property `foo` on our
      # new aggregated object schema.
      #
      # Additionally, since this is a "one of" schema, we can't make any of the properties on the
      # aggregated object schema be required, since the whole point is that they're deserialized
      # opportunistically.
      aggregated_properties = {}

      subschemas.each { |subschema|
        resolved_subschema = resolve_schema(root_schema, subschema)

        @logger.debug "ETNUV original subschema: #{subschema}"
        @logger.debug "ETNUV resolved subschema: #{resolved_subschema}"

        resolved_properties = resolved_subschema.dig('type', 'object', 'options')
        if resolved_properties.nil? || resolved_properties.keys.length != 1
          @logger.error "Encountered externally tagged enum schema with non-unit variants where the resulting \
          resolved schema for a given variant had more than one property!"
          @logger.error "Original variant subschema: #{subschema}"
          @logger.error "Resolved variant subschema: #{resolved_subschema}"
        end

        # Generate a description from the overall subschema and apply it to the properly itself. We
        # do this since it would be lost otherwise.
        description = get_rendered_description_from_schema(subschema)
        resolved_properties.each { |property_name, property_schema|
          property_schema['description'] = description unless description.empty?
          aggregated_properties[property_name] = property_schema
        }
      }

      @logger.debug "Resolved as 'externally-tagged with only non-unit variants' enum schema."

      return { '_resolved' => { 'type' => { 'object' => { 'options' => aggregated_properties } } } }
    end
  end

  # Fallback schema pattern: mixed-mode enums.
  #
  # These are enums that can basically be some combination of possible values: `Concurrency` is the
  # canonical example as it can be set via `"none"`, `"adaptive"`, or an integer between 1 and...
  # 2^64 or something like that. None of the subschemas overlap in any way.
  #
  # We just end up emitting a composite type output to cover each possibility, so the above would
  # have the `string` type with an `enum` of `"none"` and `"adaptive"`, and the uint type for the
  # integer side. This code mostly assumes the upstream schema is itself correct, in terms of not
  # providing a schema that is too ambiguous to properly validate against an input document.
  @logger.debug "Resolved as 'fallback mixed-mode' enum schema."

  @logger.debug "Tagging mode: #{enum_tagging}"
  @logger.debug "Input subschemas: #{subschemas}"

  resolved_subschemas = subschemas.filter_map { |subschema| resolve_schema(root_schema, subschema) }
  @logger.debug "Resolved fallback schemas: #{resolved_subschemas}"

  type_defs = resolved_subschemas.reduce { |acc, item| schema_aware_nested_merge(acc, item) }

  @logger.debug "Schema-aware merged result: #{type_defs}"

  { '_resolved' => { 'type' => type_defs['type'] }, 'annotations' => 'mixed_mode' }
end

def apply_schema_default_value!(source_schema, resolved_schema)
  @logger.debug "Applying schema default values."

  default_value = source_schema['default']
  unless default_value.nil?
    # Make sure that the resolved schema actually has a type definition that matches the type of the
    # given default value, since anything else would be indicative of a nasty bug in schema
    # generation.
    default_value_type = docs_type_str(default_value)
    resolved_schema_type_field = get_json_schema_type_field_for_value(source_schema, resolved_schema, default_value)
    if resolved_schema_type_field.nil?
      @logger.error "Schema has default value declared that does not match type of resolved schema: \
      \
      Source schema: #{to_pretty_json(source_schema)} \
      Default value: #{to_pretty_json(default_value)} (type: #{default_value_type}) \
      Resolved schema: #{to_pretty_json(resolved_schema)}"
      exit 1
    end

    case default_value_type
    when 'object'
      # For objects, we set the default values on a per-property basis by trying to extract the
      # value for each property from the object set as the default value. This is because of how we
      # generally render documentation for fields that are objects, where we want to show the
      # default value alongside the field itself, rather than specifying at the top level all of the
      # default values...
      #
      # In other words, instead of this:
      #
      #  buffer:
      #    default value: { type = "memory", max_events = 500 }
      #
      #    type:
      #      ...
      #    max_events:
      #      ...
      #
      # we want to get this:
      #
      #  buffer:
      #    type:
      #      default value: "memory"
      #    max_events:
      #      default value: 500
      #
      resolved_properties = resolved_schema_type_field['options']
      resolved_properties.each do |property_name, resolved_property|
        @logger.debug "Trying to apply default value for resolved property '#{property_name}'..."
        property_default_value = default_value[property_name]
        if !property_default_value.nil?
          source_property = find_nested_object_property_schema(source_schema, property_name)
          if !source_property.nil?
            # If we found the source schema for the property itself, use that to cleanly apply
            # default values to the property.
            source_property_with_default = deep_copy(source_property)
            source_property_with_default['default'] = property_default_value
            apply_schema_default_value!(source_property_with_default, resolved_property)

            resolved_property['required'] = false
          else
            # We don't have a source for the property itself, presumably because we're dealing with
            # a complex subschema, so just go based off of the type of the default value itself.
            property_type_field = get_json_schema_type_field_for_value(source_property, resolved_property, property_default_value)
            if !property_type_field.nil?
              property_type_field['default'] = property_default_value
              resolved_property['required'] = false
            end
          end
        end
      end
    else
      # We're dealing with an array or normal scalar or whatever, so just apply the default directly.
      resolved_schema_type_field['default'] = default_value
    end

    @logger.debug "Applied default value(s) to schema."
  end
end

def apply_schema_metadata!(source_schema, resolved_schema)
  # Handle marking string schemas as templateable, which shows a special blurb in the rendered
  # documentation HTML that explains what this means and links to the template syntax, etc.
  is_templateable = get_schema_metadata(source_schema, 'docs::templateable') == true
  string_type_def = resolved_schema.dig('type', 'string')
  if !string_type_def.nil? && is_templateable
    @logger.debug "Schema is templateable."
    string_type_def['syntax'] = 'template'
  end

  # TODO: Handle the niche case where we have an object schema without any of its own fields -- aka a map
  # of optional key/value pairs i.e. tags -- and it allows templateable values.

  # Handle adding any defined examples to the type definition.
  #
  # TODO: Since a resolved schema could be represented by _multiple_ input types, and we only take
  # examples in the form of strings, we don't have a great way to discern which type definition
  # should get the examples applied to it (i.e. for a number-or-enum-of-strings schema, do we
  # apply the examples to the number type def or the enum type def?) so we simply... apply them to
  # any type definition present in the resolved schema.
  #
  # We might be able to improve this in the future with a simply better heuristic, dunno. This might
  # also work totally fine as-is!
  examples = get_schema_metadata(source_schema, 'docs::examples')
  if !examples.nil?
    flattened_examples = [examples].flatten.map { |example|
      if example.is_a?(Hash)
        sort_hash_nested(example)
      else
        example
      end
    }

    @logger.debug "Schema has #{flattened_examples.length} example(s)."
    resolved_schema['type'].each { |type_name, type_def|
      # We need to recurse one more level if we're dealing with an array type definition, as we need
      # to stick the examples on the type definition for the array's `items`. There might also be
      # multiple type definitions under `items`, but we'll cross that bridge if/when we get to it.
      case type_name
      when 'array'
        type_def['items']['type'].each { |subtype_name, subtype_def|
          if subtype_name != 'array'
            subtype_def['examples'] = flattened_examples
          end
        }
      else
        type_def['examples'] = flattened_examples
      end
    }
  end

  # Apply any units that have been specified.
  type_unit = get_schema_metadata(source_schema, 'docs::type_unit')
  if !type_unit.nil?
    schema_type = numeric_schema_type(resolved_schema)
    if !schema_type.nil?
      resolved_schema['type'][schema_type]['unit'] = type_unit.to_s
    end
  end

  # Modify the `syntax` of a string type definition if overridden via metadata.
  syntax_override = get_schema_metadata(source_schema, 'docs::syntax_override')
  if !syntax_override.nil?
    if resolved_schema_type?(resolved_schema) != "string"
      @logger.error "Non-string schemas should not use the `syntax_override` metadata attribute."
      exit 1
    end

    resolved_schema['type']['string']['syntax'] = syntax_override.to_s
  end
end

# Reconciles the resolved schema, detecting and fixing any logical inconsistencies.
#
# This provides a mechanism to fix up any inconsistencies that are created during the resolution
# process that would otherwise be very complex to fix in the resolution codepath. Sometimes,
# inconsistencies are only present after resolving merged subschemas, and so on, and so this
# function serves as a spot to do such reconciliation, as it is called right before returning a
# resolved schema.
def reconcile_resolved_schema!(resolved_schema)
  @logger.debug "Reconciling resolved schema..."

  # Only works if `type` is an object, which it won't be in some cases, such as a schema that maps
  # to a cycle entrypoint, or is hidden, and so on.
  if !resolved_schema['type'].is_a?(Hash)
    @logger.debug "Schema was not a fully resolved schema; reconciliation not applicable."
    return
  end

  @logger.debug "Reconciling schema: #{to_pretty_json(resolved_schema)}"

  # If we're dealing with an object schema, run this for each of its properties.
  object_properties = resolved_schema.dig('type', 'object', 'options')
  if !object_properties.nil?
    object_properties.values.each { |resolved_property| reconcile_resolved_schema!(resolved_property) }
  else
    # Look for required/default value inconsistencies.
    #
    # One example is the `lua` transform and the `version` field. It's marked as required by the
    # version 2 configuration types, but it's optional for version 1, which allows the deserializer to
    # only deserialize things as version 1 if `version` isn't set, avoiding a backwards-incompatible
    # situation... but in this script, we only compare the subschemas in terms of their const-ness,
    # and don't look at the `required` portion.
    #
    # This means that we can generate output for a field that says it has a default value of `null`
    # but is a required field, which is a logical inconsistency in terms of the Cue schema where we
    # import the generated output of this script: it doesn't allow setting a default value for a field
    # if the field is required, and vice versa.
    if resolved_schema['required']
      # For all schema type fields, see if they have a default value equal to `nil`. If so, remove
      # the `default` field entirely.
      resolved_schema['type'].keys.each { |type_field|
        type_field_default_value = resolved_schema['type'][type_field].fetch('default', :does_not_exist)
        if type_field_default_value.nil?
          @logger.debug "Removing null `default` field for type field '#{type_field}'..."

          resolved_schema['type'][type_field].delete('default')
        end
      }
    end

    # Look for merged string const values that need to become an enum.
    #
    # As part of our enum schema resolving, we have a fallback mode where we resolve each subschema
    # and merge them together in a nested fashion, under the assumption that they don't overlap in
    # an invalid way i.e. same field in two schemas but with differing/incompatible types.
    #
    # This works, but one area it falls down is where a field is a `const` in different subschemas,
    # where even if the value is the same type for all overlaps of `const`, the normal nested merge
    # would result in a last-write-wins for that field. We compensate for this by using a
    # schema-aware nested merge, where if we're merging a field called `const`, we turn it into a
    # array of the const data, which includes the const value itself and the description of the enum
    # variant.
    #
    # The final step would be to change from `const` to `enum`, because Cue doesn't recognize the
    # `const` type, regardless of whether it's a single value or a map of key/value pairs. We cannot
    # do that in the merge, however, because there's no way to specify a new resulting key to use,
    # only how the values should be merged.
    #
    # Thus, we handle that here by looking for any `const` field that has an array value, turning it
    # into a map of const value to enum variant description. Since no normal schema would have a
    # `const` value that was an array value to begin with, we can safely search for such instances
    # and confidently know that the field can be renamed from `const` to `enum`.
    resolved_schema['type'].keys.each { |type_field|
      const_type_field = resolved_schema.dig('type', type_field, 'const')
      if !const_type_field.nil?
        @logger.debug "Converting `const` values to `enum` for type field '#{type_field}'..."
        @logger.debug "Resolved schema: #{resolved_schema}"

        enum_values = if const_type_field.is_a?(Array)
          const_type_field
            .map { |const| [const['value'], const['description']] }
        else
          # If the value isn't already an array, we'll create the enum values map directly.
          { const_type_field['value'] => const_type_field['description'] }
        end

        @logger.debug "Reconciled enum values for const: #{enum_values}"

        resolved_schema['type'][type_field].delete('const')
        resolved_schema['type'][type_field]['enum'] = enum_values
      end
    }

    # Push the schema description into the type field for string consts.
    #
    # As part of resolving const schemas, we need to use their descriptions when eventually
    # converting them to an enum schema that is supported on the Cue side. This implies the const
    # value becoming a key in a map, whose value is the description of the const schema.
    #
    # We do that here because it's simpler to not have to special case the addition of the
    # description when resolving a const schema, as we do so uniformly as part of the final steps of
    # resolving a schema in general, but before reconciliation is triggered.
    resolved_schema['type'].keys.each { |type_field|
      const_type_field = resolved_schema.dig('type', type_field, 'const')
      if !const_type_field.nil? && !const_type_field.is_a?(Array)
        @logger.debug "Adding schema description to `const` type field '#{type_field}'..."

        schema_description = resolved_schema['description']
        const_type_field['description'] = schema_description unless schema_description.nil?
      end
    }
  end

  @logger.debug "Reconciled resolved schema."
end

def get_rendered_description_from_schema(schema)
  # Grab both the raw description and raw title, and if the title is empty, just use the
  # description, otherwise concatenate the title and description with newlines so that there's a
  # whitespace break between the title and description.
  raw_description = schema.fetch('description', '')
  raw_title = schema.fetch('title', '')

  description = raw_title.empty? ? raw_description : "#{raw_title}\n\n#{raw_description}"
  description.strip
end

def unwrap_resolved_schema(root_schema, schema_name, friendly_name)
  @logger.info "[*] Resolving schema definition for #{friendly_name}..."

  # Try and resolve the schema, unwrapping it as an object schema which is a requirement/expectation
  # of all component-level schemas. We additionally sort all of the object properties, which makes
  # sure the docs are generated in alphabetical order.
  resolved_schema = resolve_schema_by_name(root_schema, schema_name)

  unwrapped_resolved_schema = resolved_schema.dig('type', 'object', 'options')
  if unwrapped_resolved_schema.nil?
    @logger.error 'Configuration types must always resolve to an object schema.'
    exit 1
  end

  return sort_hash_nested(unwrapped_resolved_schema)
end

def render_and_import_schema(unwrapped_resolved_schema, friendly_name, config_map_path, cue_relative_path)

  # Set up the appropriate structure for the value based on the configuration map path. It defines
  # the nested levels of the map where our resolved schema should go, as well as a means to generate
  # an appropriate prefix for our temporary file.
  data = {}
  last = data
  config_map_path.each do |segment|
    last[segment] = {} if last[segment].nil?

    last = last[segment]
  end

  last['configuration'] = unwrapped_resolved_schema

  config_map_path.prepend('config-schema-base')
  tmp_file_prefix = config_map_path.join('-')

  final_json = to_pretty_json(data)

  # Write the resolved schema as JSON, which we'll then use to import into a Cue file.
  json_output_file = write_to_temp_file(["config-schema-#{tmp_file_prefix}-", '.json'], final_json)
  @logger.info "[✓]   Wrote #{friendly_name} schema to '#{json_output_file}'. (#{final_json.length} bytes)"

  # Try importing it as Cue.
  @logger.info "[*] Importing #{friendly_name} schema as Cue file..."
  cue_output_file = "website/cue/reference/#{cue_relative_path}"
  unless system(@cue_binary_path, 'import', '-f', '-o', cue_output_file, '-p', 'metadata', json_output_file)
    @logger.error "[!]   Failed to import #{friendly_name} schema as valid Cue."
    exit 1
  end
  @logger.info "[✓]   Imported #{friendly_name} schema to '#{cue_output_file}'."
end

def render_and_import_base_component_schema(root_schema, schema_name, component_type)
  friendly_name = "base #{component_type} configuration"
  unwrapped_resolved_schema = unwrap_resolved_schema(root_schema, schema_name, friendly_name)
  render_and_import_schema(
    unwrapped_resolved_schema,
    friendly_name,
    ["base", "components", "#{component_type}s"],
    "components/base/#{component_type}s.cue"
  )
end

def render_and_import_component_schema(root_schema, schema_name, component_type, component_name)
  friendly_name = "'#{component_name}' #{component_type} configuration"
  unwrapped_resolved_schema = unwrap_resolved_schema(root_schema, schema_name, friendly_name)
  render_and_import_schema(
    unwrapped_resolved_schema,
    friendly_name,
    ["base", "components", "#{component_type}s", component_name],
    "components/#{component_type}s/base/#{component_name}.cue"
  )
end

def render_and_import_base_api_schema(root_schema, apis)
  api_schema = {}
  apis.each do |component_name, schema_name|
    friendly_name = "'#{component_name}' #{schema_name} configuration"
    resolved_schema = unwrap_resolved_schema(root_schema, schema_name, friendly_name)
    api_schema[component_name] = resolved_schema
  end

  render_and_import_schema(
    api_schema,
    "configuration",
    ["base", "api"],
    "base/api.cue"
  )
end

def render_and_import_base_global_option_schema(root_schema, global_options)
  global_option_schema = {}

  global_options.each do |component_name, schema_name|
    friendly_name = "'#{component_name}' #{schema_name} configuration"

    if component_name == "global_option"
      # Flattening global options
      unwrap_resolved_schema(root_schema, schema_name, friendly_name)
        .each { |name, schema| global_option_schema[name] = schema }
    else
      # Resolving and assigning other global options
      global_option_schema[component_name] = resolve_schema_by_name(root_schema, schema_name)
    end
  end

  render_and_import_schema(
    global_option_schema,
    "configuration",
    ["base", "configuration"],
    "base/configuration.cue"
  )
end

if ARGV.empty?
  puts 'usage: extract-component-schema.rb <configuration schema path>'
  exit 1
end

# Ensure that Cue is present since we need it to import our intermediate JSON representation.
if @cue_binary_path.nil?
  puts 'Failed to find \'cue\' binary on the current path. Install \'cue\' (or make it available on the current path) and try again.'
  exit 1
end

schema_path = ARGV[0]
root_schema = JSON.parse(File.read(schema_path))

component_types = %w[source transform sink]

# First off, we generate the component type configuration bases. These are the high-level
# configuration settings that are universal on a per-component type basis.
#
# For example, the "base" configuration for a sink would be the inputs, buffer settings, healthcheck
# settings, and proxy settings... and then the configuration for a sink would be those, plus
# whatever the sink itself defines.
component_bases = root_schema['definitions'].filter_map do |key, definition|
  component_base_type = get_schema_metadata(definition, 'docs::component_base_type')
  { component_base_type => key } if component_types.include? component_base_type
end
.reduce { |acc, item| nested_merge(acc, item) }

component_bases.each do |component_type, schema_name|
  render_and_import_base_component_schema(root_schema, schema_name, component_type)
end

# Now we'll generate the base configuration for each component.
all_components = root_schema['definitions'].filter_map do |key, definition|
  component_type = get_schema_metadata(definition, 'docs::component_type')
  component_name = get_schema_metadata(definition, 'docs::component_name')
  { component_type => { component_name => key } } if component_types.include? component_type
end
.reduce { |acc, item| nested_merge(acc, item) }

all_components.each do |component_type, components|
  components.each do |component_name, schema_name|
    render_and_import_component_schema(root_schema, schema_name, component_type, component_name)
  end
end

apis = root_schema['definitions'].filter_map do |key, definition|
  component_type = get_schema_metadata(definition, 'docs::component_type')
  component_name = get_schema_metadata(definition, 'docs::component_name')
  { component_name => key } if component_type == "api"
end
.reduce { |acc, item| nested_merge(acc, item) }

render_and_import_base_api_schema(root_schema, apis)


# At last, we generate the global options configuration.
global_options = root_schema['definitions'].filter_map do |key, definition|
  component_type = get_schema_metadata(definition, 'docs::component_type')
  component_name = get_schema_metadata(definition, 'docs::component_name')
  { component_name => key } if component_type == "global_option"
end
.reduce { |acc, item| nested_merge(acc, item) }

render_and_import_base_global_option_schema(root_schema, global_options)
