#!/usr/bin/env ruby
# frozen_string_literal: true

begin
  require 'json'
  require 'logging'
  require 'tempfile'
rescue LoadError => e
  puts "Load error: #{e.message}"
  exit
end

@logger = Logging.logger['default']
@logger.add_appenders(Logging.appenders.stdout(
  'stdout',
  layout: Logging.layouts.pattern(
    pattern: '[%d] %l %m\n',
    color_scheme: 'default'
  )
))
@logger.level = ENV['LOG_LEVEL'] || 'info'

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

# Helpers for caching resolved schemas and detecting schema resolution cycles.
@resolved_schema_cache = {}
@schema_resolution_queue = {}

def add_to_schema_resolution_stack(schema_name)
  @schema_resolution_queue[schema_name] = true
end

def remove_from_schema_resolution_stack(schema_name)
  @schema_resolution_queue.delete(schema_name)
end

def schema_resolution_cycle?(schema_name)
  @schema_resolution_queue.key?(schema_name)
end

# Gets the schema of the given `name` from the resolved schema cache, if it exists.
def get_cached_resolved_schema(schema_name)
  @resolved_schema_cache[schema_name]
end

# Generic helpers for making working with Ruby a bit easier.
def deep_copy(obj)
  Marshal.load(Marshal.dump(obj))
end

def mergeable?(value)
  value.is_a?(Hash) || value.is_a?(Array)
end

def nested_merge(base, override)
  # Handle some basic cases first.
  if base.nil?
    return override
  elsif override.nil?
    return base
  elsif !mergeable?(base) && !mergeable?(override)
    return override
  end

  merger = proc { |_, v1, v2|
    if v1.is_a?(Hash) && v2.is_a?(Hash)
      v1.merge(v2, &merger)
    elsif v1.is_a?(Array) && v2.is_a?(Array)
      v1 | v2
    else
      [:undefined, nil, :nil].include?(v2) ? v1 : v2
    end
  }
  base.merge(override.to_h, &merger)
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
  elsif value.is_a?(Numeric)
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
def get_schema_type_field_for_value(schema, value)
  value_type = docs_type_str(value)
  case value_type
  when 'number'
    %w[uint int float].each do |numeric_type|
      type_field = schema.dig('type', numeric_type)
      return type_field unless type_field.nil?
    end
  else
    schema.dig('type', value_type)
  end
end

def get_schema_metadata(schema, key)
  schema.dig('_metadata', key)
end

def get_schema_ref(schema)
  schema['$ref']
end

# Gets the schema type for the given schema.
def get_schema_type(schema)
  if schema.key?('allOf')
    'all-of'
  elsif schema.key?('oneOf')
    'one-of'
  elsif schema.key?('type')
    schema['type']
  elsif schema.key?('const')
    'const'
  elsif schema.key?('enum')
    'enum'
  end
end

# Gets a schema definition from the root schema, by name.
def get_schema_by_name(root_schema, schema_name)
  schema_name = schema_name.gsub(%r{#/definitions/}, '')
  schema_def = root_schema.dig('definitions', schema_name)
  if schema_def.nil?
    @logger.error "Could not find schema definition '#{schema_name}' in given schema."
    exit
  end

  schema_def
end

# Applies various fields to an object property.
#
# This includes items such as any default value that is present, or whether or not the property is
# required.
def apply_object_property_fields!(parent_schema, property_schema, property_name, property)
  @logger.debug "Applying object property fields for '#{property_name}'..."

  # Set whether or not this property is required.
  required_properties = parent_schema['required'] || []
  has_default_value = !property_schema['default'].nil? || !parent_schema.dig('default', property_name).nil?
  property['required'] = required_properties.include?(property_name) && !has_default_value
end

# Resolves a schema reference, if present.
#
# If the given schema in fact is a reference to a schema definition, it is retrieved and merged into
# the given schema, and the reference field is removed.
#
# For any overlapping fields in the given schema and the referenced schema, the fields from the
# given schema will win.
def resolve_schema_reference(root, schema)
  schema = deep_copy(schema)

  until schema['$ref'].nil?
    resolved_schema = get_schema_by_name(root, schema['$ref'])

    schema.delete('$ref')
    schema = nested_merge(schema, resolved_schema)
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

  # TODO: We may or may not have to also do reduction at further levels i.e. clean up `properties`
  # when the schema has the object type, etc.

  allowed_properties = ['type', 'const', 'enum', 'allOf', 'oneOf', '$ref', 'items']
  schema.delete_if { |key, _value| !allowed_properties.include?(key) }

  schema['allOf'].map!(get_reduced_schema) if schema.key?('allOf')

  schema['oneOf'].map!(get_reduced_schema) if schema.key?('oneOf')

  schema
end

# Fully resolves a schema definition, if it exists.
#
# This looks up a schema definition by the given `name` within `root_schema` and resolves it, if it
# exists, Otherwise, `nil` is returned.
#
# Resolved schemas are cached.
#
# See `resolve_schema` for more details.
def resolve_schema_by_name(root_schema, schema_name)
  # If it's already cached, use that.
  resolved = get_cached_resolved_schema(schema_name)
  return deep_copy(resolved) unless resolved.nil?

  if schema_resolution_cycle?(schema_name)
    @logger.error "Cycle detected while resolving schema '#{schema_name}'. \
    \
    Cycles must be broken manually at the source code level by annotating fields that induce \
    cycles with `#[configurable(metadata(docs::cycle_entrypoint))]`. As such a field will have no type \
    information rendered, it is advised to supply a sufficiently detailed field description that \
    describes the allowable values, etc."
    exit
  end

  # It wasn't already cached, so we actually have to resolve it.
  schema = get_schema_by_name(root_schema, schema_name)
  add_to_schema_resolution_stack(schema_name)
  resolved = resolve_schema(root_schema, schema)
  remove_from_schema_resolution_stack(schema_name)
  @resolved_schema_cache[schema_name] = resolved
  deep_copy(resolved)
end

# Fully resolves the schema.
#
# This recursively resolves schema references, as well as flattening them into a single object, and
# transforming certain usages -- composite/enum (`allOf`, `oneOf`), etc -- into more human-readable
# forms.
def resolve_schema(root_schema, schema)
  # First, skip any schema that is marked as hidden.
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

  # Next, return a bare-minimum resolved schema for any schema marked as a cycle entrypoint.
  #
  # This means the schema is self-referential (i.e. the `pipelines` transform, which is part of
  # `Transforms`, having a field that references `Transforms`) and we have to break the cycle.
  #
  # We have to return _something_, as it's a real part of the schema, so we just return a basic
  # schema with no type information but with any description that is specified, etc.
  if get_schema_metadata(schema, 'docs::cycle_entrypoint')
    resolved = { 'type' => 'blank' }
    description = get_rendered_description_from_schema(schema)
    resolved['description'] = description unless description.empty?
    return resolved
  end

  # Figure out if we're resolving a bare schema, or a reference to an existing schema.
  #
  # If it is, we resolve _that_ schema first, which may also involve getting a cached version of the
  # resolved schema. This is useful from a performance perspective, but primarily serves as a way to
  # break cycles when resolving self-referential schemas, such as the `pipelines` transform.
  #
  # Otherwise, we simply resolve the schema as it exists.
  referenced_schema_name = get_schema_ref(schema)
  resolved = if !referenced_schema_name.nil?
    resolve_schema_by_name(root_schema, referenced_schema_name)
  else
    resolve_bare_schema(root_schema, schema)
  end

  # Apply any necessary defaults, descriptions, etc, to the resolved schema. This must happen here
  # because there could be callsite-specific overrides to defaults, descriptions, etc, for a given
  # schema definition that have to be layered.
  apply_schema_default_value!(schema, resolved)
  apply_schema_metadata!(schema, resolved)

  @logger.debug "About to resolve original schema: #{schema}"

  description = get_rendered_description_from_schema(schema)
  resolved['description'] = description unless description.empty?
  resolved
end

# Fully resolves a bare schema.
#
# A bare schema is one that has no references to another schema, etc.
def resolve_bare_schema(root_schema, schema)
  resolved = case get_schema_type(schema)
    when 'all-of'
      @logger.debug 'Resolving composite schema.'

      # Composite schemas are indeed the sum of all of their parts, so resolve each subschema and
      # merge their resolved state together.
      reduced = schema['allOf'].filter_map { |subschema| resolve_schema(root_schema, subschema) }
                              .reduce { |acc, item| nested_merge(acc, item) }
      reduced['type']
    when 'one-of'
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
        resolved_additional_properties = resolve_schema(root_schema, additional_properties)
        options.push(['*', resolved_additional_properties])
      end

      { 'object' => { 'options' => options.to_h } }
    when 'string'
      @logger.debug 'Resolving string schema.'

      string_def = { 'syntax' => 'literal' }
      string_def['default'] = schema['default'] unless schema['default'].nil?

      { 'string' => string_def }
    when 'number'
      @logger.debug 'Resolving number schema.'

      numeric_type = get_schema_metadata(schema, 'docs::numeric_type') || 'number'
      number_def = {}
      number_def['default'] = schema['default'] unless schema['default'].nil?

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
      const_value = schema['const']
      const_type = docs_type_str(const_value)
      { const_type => { 'const' => const_value } }
    when 'enum'
      @logger.debug 'Resolving enum const schema.'

      # Similarly to `const` schemas, `enum` schemas are merely multiple possible constant values. Given
      # that JSON Schema does allow for the constant values to differ in type, we group them all by
      # type to get the resolved output.
      enum_values = schema['enum']
      grouped = enum_values.group_by { |value| docs_type_str(value) }
      grouped.transform_values! { |values| { 'enum' => values } }
      grouped
    else
      @logger.error "Failed to resolve the schema. Schema: #{schema}"
      exit
    end

  { 'type' => resolved }
end

def resolve_enum_schema(root_schema, schema)
  subschemas = schema['oneOf']
  subschema_count = subschemas.count

  # Collect all of the tagging mode information upfront.
  enum_tagging = get_schema_metadata(schema, 'docs::enum_tagging')
  if enum_tagging.nil?
    @logger.error 'Enum schemas should never be missing the metadata for the enum tagging mode.'
    @logger.error "Schema: #{JSON.pretty_generate(schema)}"
    exit
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
        if get_schema_type(single_subschema) == json_type_str(schema['default'])
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
      resolve_schema(root_schema, subschema) if get_schema_type(subschema) == 'object'
    end

    if resolved_subschemas.count == subschemas.count
      @logger.debug "Detected likely 'internally-tagged with named fields' enum schema, applying further validation..."

      unique_resolved_properties = {}
      unique_tag_values = {}

      resolved_subschemas.each do |resolved_subschema|
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

        %w[string number boolean].each do |allowed_type|
          maybe_tag_value = tag_subschema.dig('type', allowed_type, 'const')
          unless maybe_tag_value.nil?
            tag_value = maybe_tag_value
            break
          end
        end

        @logger.debug "Tag value of #{tag_value}, with original resolved schema: #{resolved_subschema}"

        if tag_value.nil?
          @logger.error 'All enum subschemas representing an internally-tagged enum must have the tag field use a const value.'
          @logger.error "Tag subschema: #{JSON.pretty_generate(tag_subschema)}"
          exit
        end

        if unique_tag_values.key?(tag_value)
          @logger.error "Found duplicate tag value '#{tag_value}' when resolving enum subschemas."
          exit
        end

        unique_tag_values[tag_value] = tag_subschema

        # Now merge all of the properties from the given subschema, so long as the overlapping
        # properties have the same schema.
        resolved_subschema_properties.each do |property_name, property_schema|
          existing_property = unique_resolved_properties[property_name]
          resolved_property = if !existing_property.nil?
            # The property is already being tracked, so just do a check to make sure the property from our
            # current subschema matches the existing property, schema-wise, before we update it.
            reduced_existing_property = get_reduced_schema(existing_property)
            reduced_new_property = get_reduced_schema(property_schema)

            if reduced_existing_property != reduced_new_property
              @logger.error "Had overlapping property '#{property_name}' from resolved enum subschema, but schemas differed: \
              Existing property schema (reduced): #{reduced_existing_property} \
              New property schema (reduced): #{reduced_new_property}"
              exit
            end

            # The schemas match, so just update the list of "relevant when" values.
            existing_property['relevant_when'].push(tag_value)
            existing_property
          else
            # First time seeing this particular property.
            property_schema['relevant_when'] = [tag_value]
            property_schema
          end

          unique_resolved_properties[property_name] = resolved_property
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
      unique_resolved_properties[enum_tag_field] = {
        'required' => true,
        'type' => {
          'string' => {
            'enum' => unique_tag_values.transform_values do |tag_schema|
              @logger.debug "Tag schema: #{tag_schema}"
              get_rendered_description_from_schema(tag_schema)
            end
          }
        }
      }

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
      @logger.debug "Resolved as 'externally-tagged with only unit varaints' enum schema."

      return { '_resolved' => { 'type' => { 'string' => {
        'enum' => tag_values.transform_values { |tag_schema| get_rendered_description_from_schema(tag_schema) }
      } } } }
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
  type_defs = subschemas.filter_map { |subschema| resolve_schema(root_schema, subschema) }
                        .reduce { |acc, item| nested_merge(acc, item) }

  @logger.debug "Resolved as 'fallback mixed-mode' enum schema."

  { '_resolved' => { 'type' => type_defs['type'] }, 'annotations' => 'mixed_mode' }
end

def apply_schema_default_value!(source_schema, resolved_schema)
  @logger.debug "Apply defaults to schema."
  @logger.debug "Source schema: #{source_schema}"
  @logger.debug "Resolved schema: #{resolved_schema}"

  default_value = source_schema['default']
  unless default_value.nil?
    # Make sure that the resolved schema actually has a type definition that matches the type of the
    # given default value, since anything else would be indicative of a nasty bug in schema
    # generation.
    default_value_type = docs_type_str(default_value)
    resolved_schema_type_field = get_schema_type_field_for_value(resolved_schema, default_value)
    if resolved_schema_type_field.nil?
      @logger.error "Schema has default value declared that does not match type of resolved schema: \
      \
      Source schema: #{source_schema} \
      Default value: #{default_value} (type: #{default_value_type}) \
      Resolved schema: #{resolved_schema}"
      exit
    end

    case default_value_type
    when 'array'
      # We blindly set the default values without verifying that they match the type of the schema
      # described by `items`. This might need to be more rigid in the future, but we're just going
      # with it for now.

      # TODO: It should technically be as easy as just verifying that every item in `default_value`
      # matches the schema type of whatever is under `items`, I believe?
      resolved_schema_type_field['default'] = default_value
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
        property_default_value = default_value[property_name]
        property_type_field = get_schema_type_field_for_value(resolved_property, property_default_value)
        property_type_field['default'] = property_default_value unless property_type_field.nil?
      end
    else
      # We're dealing with a normal scalar or whatever, so just apply the default directly.
      @logger.debug "Resolved schema type field: #{resolved_schema_type_field}"
      @logger.debug "Default value on fallback path: #{default_value}"
      resolved_schema_type_field['default'] = default_value
    end
  end
end

def apply_schema_metadata!(source_schema, resolved_schema)
  # Handle marking string schemas as templateable, which shows a special blurb in the rendered
  # documentation HTML that explains what this means and links to the template syntax, etc.
  is_templateable = get_schema_metadata(source_schema, 'docs::templateable') == true
  string_type_def = resolved_schema.dig('type', 'string')
  if !string_type_def.nil? && is_templateable
    string_type_def['syntax'] = 'template'
  end

  # Handle the niche case where we have an object schema without any of its own fields -- aka a map
  # of optional key/value pairs i.e. tags -- and it allows templateable values.
end

def get_rendered_description_from_schema(schema)
  # If the schema is marked as `no_description`, we're being told -- for whatever reason -- that the
  # existing title/description should not be rendered in the output. This is primarily to avoid
  # spitting out developer-oriented documentation into the user-facing documentation, when we're
  # providing the necessary description in another way.
  if !get_schema_metadata(schema, 'docs::no_description').nil?
    return ''
  end

  # Grab both the raw description and raw title, and if the title is empty, just use the
  # description, otherwise concatenate the title and description with newlines so that there's a
  # whitespace break between the title and description.
  raw_description = schema.fetch('description', '')
  raw_title = schema.fetch('title', '')

  description = raw_title.empty? ? raw_description : "#{raw_title}\n\n#{raw_description}"
  description.strip
end

def render_and_import_schema(root_schema, schema_name, friendly_name, config_map_path, cue_relative_path)
  @logger.info "[*] Resolving schema definition for #{friendly_name}..."

  # Try and resolve the schema, unwrapping it as an object schema which is a requirement/expectation
  # of all component-level schemas. We additionally sort all of the object properties, which makes
  # sure the docs are generated in alphabetical order.
  resolved_schema = resolve_schema_by_name(root_schema, schema_name)

  unwrapped_resolved_schema = resolved_schema.dig('type', 'object', 'options')
  if unwrapped_resolved_schema.nil?
    @logger.error 'Configuration types must always resolve to an object schema.'
    exit
  end

  unwrapped_resolved_schema = sort_hash_nested(unwrapped_resolved_schema)

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

  final = { 'base' => { 'components' => data } }
  final_json = JSON.pretty_generate(final)

  # Write the resolved schema as JSON, which we'll then use to import into a Cue file.
  json_output_file = write_to_temp_file(["config-schema-#{tmp_file_prefix}-", '.json'], final_json)
  @logger.info "[✓]   Wrote #{friendly_name} schema to '#{json_output_file}'. (#{final_json.length} bytes)"

  # Try importing it as Cue.
  @logger.info "[*] Importing #{friendly_name} schema as Cue file..."
  cue_output_file = "website/cue/reference/components/#{cue_relative_path}"
  unless system(@cue_binary_path, 'import', '-f', '-o', cue_output_file, '-p', 'metadata', json_output_file)
    @logger.error "[!]   Failed to import #{friendly_name} schema as valid Cue."
    exit
  end
  @logger.info "[✓]   Imported #{friendly_name} schema to '#{cue_output_file}'."
end

def render_and_import_base_component_schema(root_schema, schema_name, component_type)
  render_and_import_schema(
    root_schema,
    schema_name,
    "base #{component_type} configuration",
    [component_type],
    "base/#{component_type}.cue"
  )
end

def render_and_import_component_schema(root_schema, schema_name, component_type, component_name)
  render_and_import_schema(
    root_schema,
    schema_name,
    "'#{component_name}' #{component_type} configuration",
    [component_type, component_name],
    "#{component_type}s/base/#{component_name}.cue"
  )
end

if ARGV.empty?
  puts 'usage: extract-component-schema.rb <configuration schema path>'
  exit
end

# Ensure that Cue is present since we need it to import our intermediate JSON representation.
if @cue_binary_path.nil?
  puts 'Failed to find \'cue\' binary on the current path. Install \'cue\' (or make it available on the current path) and try again.'
  exit
end

schema_path = ARGV[0]
root_schema = JSON.load_file schema_path

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
