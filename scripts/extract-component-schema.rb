#!/usr/bin/env ruby

begin
  require "json"
rescue LoadError => ex
  puts "Load error: #{ex.message}"
  exit
end

def nested_merge(base, override)
  merger = proc { |_, v1, v2| Hash === v1 && Hash === v2 ? v1.merge(v2, &merger) : Array === v1 && Array === v2 ? v1 | v2 : [:undefined, nil, :nil].include?(v2) ? v1 : v2 }
  base.merge(override.to_h, &merger)
end

# Gets the JSON Schema-compatible type name for the given Ruby Value.
def type_str(value)
  if value.is_a?(String)
    "string"
  elsif value.is_a?(Numeric)
    "number"
  elsif [true, false].include?(value)
    "boolean"
  elsif value.is_a?(Array)
    "array"
  elsif value.is_a?(Hash)
    "object"
  else
    "null"
  end
end

# Gets the schema type for the given schema.
def get_schema_type(schema)
  if schema.has_key?("allOf")
    "all-of"
  elsif schema.has_key?("oneOf")
    "one-of"
  elsif schema.has_key?("type")
    schema["type"]
  elsif schema.has_key?("const")
    "const"
  elsif schema.has_key?("enum")
    "enum"
  end
end

# Gets a schema definition from the root schema, by name.
def get_schema_by_name(root_schema, schema_name)
  schema_name = schema_name.gsub(/#\/definitions\//, "")
  schema_def = root_schema.dig("definitions", schema_name)
  if schema_def.nil?
    puts "Could not find schema definition '#{schema_name}' in given schema."
    exit
  end

  schema_def
end

# Resolves a schema reference, if present.
#
# If the given schema in fact is a reference to a schema definition, it is retrieved and merged into
# the given schema, and the reference field is removed.
#
# For any overlapping fields in the given schema and the referenced schema, the fields from the
# given schema will win.
def resolve_schema_reference(root, schema)
  while !schema["$ref"].nil?
    resolved_schema = get_schema_by_name(root, schema["$ref"])

    current_schema = schema.clone
    current_schema.delete("$ref")
    schema = resolved_schema.merge(current_schema)
  end

  schema
end

# Fully resolves the schema.
#
# This recursively resolves schema references, as well as flattening them into a single object, and
# transforming certain usages -- composite/enum (`allOf`, `oneOf`), etc -- into more human-readable
# forms.
def resolve_schema(root_schema, schema)
  puts "Schema: #{schema}"

  # First, skip any schema that is marked to be skipped. We use this, primarily, to influence
  # resolution for split definitions such as components.
  #
  # For example, the configuration of a sink is the sum of the "base" sink properties -- inputs,
  # buffer, etc -- and the enum subschema represented by the sink-specific Big Enum (tm)... but we
  # don't want to resolve that _into_ the definition of `SinkOuter` because we'll do that manually
  # at a later point.
  if schema.dig("_metadata", "config_docs_skip")
    return
  end

  # If this schema references another schema definition, resolve that schema definition and merge
  # it back into our schema, flattening it out.
  schema = resolve_schema_reference(root_schema, schema)

  # Now simply resolve the schema, depending on what type it is.
  resolved = case get_schema_type(schema)
    when "all-of"
      # Composite schemas are indeed the sum of all of their parts, so resolve each subschema and
      # merge their resolved state together.
      reduced = schema["allOf"].filter_map { |subschema| resolve_schema(root_schema, subschema) }
        .reduce { |acc, item| nested_merge(acc, item) }
      reduced["type"]
    when "one-of"
      # We completely defer resolution of enum schemas to `resolve_enum_schema` because there's a
      # lot of tricks and matching we need to do to suss out patterns that can be represented in more
      # condensed resolved forms.
      resolve_enum_schema(root_schema, schema)
    when "array"
      { "array" => { "items" => resolve_schema(root_schema, schema["items"]) } }
    when "object"
      # TODO: Not all objects have an actual set of properties, such as anything using
      # `additionalProperties` to allow for arbitrary key/values to be set, which is why we're
      # handling the case of nothing in `properties`... but we probably want to be able to better
      # handle expressing this in the output.. or maybe it doesn't matter, dunno!
      required_properties = schema["required"] || []
      properties = schema["properties"] || {}

      options = properties.filter_map { |property_name, property_schema|
        resolved_property = resolve_schema(root_schema, property_schema)
        if !resolved_property.nil?
          resolved_property["required"] = required_properties.include?(property_name)

          [property_name, resolved_property]
        end
      }

      { "object" => { "options" => options.to_h } }
    when "string"
      string_def = {}
      string_def["default"] = schema["default"] if !schema["default"].nil?
  
      { "string" => string_def }
    when "number"
      number_def = {}
      number_def["default"] = schema["default"] if !schema["default"].nil?
  
      { "number" => number_def }
    when "boolean"
      bool_def = {}
      bool_def["default"] = schema["default"] if !schema["default"].nil?
  
      { "bool" => bool_def }
    when "const"
      # For `const` schemas, just figure out the type of the constant value so we can generate the
      # resolved output.
      const_value = schema["const"]
      const_type = type_str(const_value)
      { const_type => { "const" => const_value } }
    when "enum"
      # Similarly to `const` schemas, `enum` schemas are merely multiple possible constant values. Given
      # that JSON Schema does allow for the constant values to differ in type, we group them all by
      # type to get the resolved output.
      enum_values = schema["enum"]
      grouped = enum_values.group_by { |value| type_str(value) }
      grouped.transform_values! { |values| { "enum" => values } }
      grouped
    else
      # We might be dealing with a "resolved" schema, which is where we wrap another layer around a
      # schema so that we can bundle annotations/metadata next to it for allowing downstream code to
      # additionally modify the resulting output.
      #
      # If we are, then handle it, otherwise... yell loudly.
      if schema.has_key?("_resolved")
        puts "  Property schema was resolved with annotations, unwrapping..."
  
        # "Resolved" schemas should already be in the shape that we would otherwise be returning here,
        # so we actually just need to unnest them by a single level to maintain that.
  
        # TODO: Where should we be dealing with annotations? For the buffer type stuff, we would need to
        # be pushing that back up to `resolve_schema` so that it could adjust the resulting
        # description, etc.
  
        schema.dig("_resolved", "type")
      else
        puts "Failed to resolve the schema. Schema: #{schema}"
        exit
      end
    end

  { "type" => resolved }
end

# Gets a reduced version of a schema.
#
# The reduced version strips out extraneous fields from the given schema, such that a value should
# be returned that is suitable for comparison with other schemas, to determine if the schemas --
# specifically the values that are allowed/valid -- are the same, while ignoring things like titles
# and descriptions.
def get_reduced_schema(schema)
  # TODO: We may or may not have to also do reduction at further levels i.e. clean up `properties`
  # when the schema has the object type, etc.

  allowed_properties = ["type", "const", "enum", "allOf", "oneOf", "$ref"]
  schema.delete_if { |key, value| !allowed_properties.include?(key) }

  if schema.has_key?("allOf")
    schema["allOf"].map!(get_reduced_schema)
  end

  if schema.has_key?("oneOf")
    schema["oneOf"].map!(get_reduced_schema)
  end

  schema
end

def resolve_enum_schema(root_schema, schema)
  subschemas = schema["oneOf"]
  subschema_count = subschemas.count

  # Collect all of the tagging mode information upfront.
  enum_tagging = schema.dig("_metadata", "enum_tagging")
  if enum_tagging.nil?
    puts "Enum schemas should never be missing the metadata for the enum tagging mode."
    exit
  end

  enum_tag_field = schema.dig("_metadata", "enum_tag_field")
  enum_content_field = schema.dig("_metadata", "enum_content_field")

  # Schema pattern: X or array of X.
  #
  # We employ this pattern on the Vector side to allow specifying a single instance of X -- object,
  # string, whatever -- or as an array of X. We just need to inspect both schemas to make sure one
  # is an array of X and the other is the same as X, or vise versa.
  if subschema_count == 2
    array_idx = subschemas.index { |subschema| subschema["type"] == "array" }
    if !array_idx.nil?
      single_idx = if array_idx == 0 then 1 else 0 end

      # We 'reduce' the subschemas which strips out all things that aren't fundamental to describing
      # the shape of what the schema accepts, so no title or description or default values, and so on.
      single_reduced_subschema = get_reduced_schema(subschemas[single_idx])
      array_reduced_subschema = get_reduced_schema(subschemas[array_idx])

      if single_reduced_subschema == array_reduced_subschema["items"]
        # The single subschema and the subschema for the array items are a match! We'll resolve this
        # as the schema of the "single" option, but with an annotation that it can be specified
        # multiple times.
        resolved_subschema = resolve_schema(root_schema, subschemas[single_idx])
        return { "_resolved" => resolved_subschema, "annotations" => "single_or_array" }
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
  if enum_tagging == "internal"
    # This transformation only makes sense when all subschemas are objects, so only resolve the ones
    # that are objects, and only proceed if all of them were resolved.
    resolved_subschemas = subschemas.filter_map { |subschema|
      resolve_schema(root_schema, subschema) if get_schema_type(subschema) == "object"
    }

    if resolved_subschemas.count == subschemas.count
      unique_resolved_properties = {}
      unique_tag_values = {}

      resolved_subschemas.each { |resolved_subschema|
        # Unwrap the resolved subschema since we only want to interact with the definition, not the
        # wrapper boilerplate.
        resolved_subschema = resolved_subschema.dig("type", "object", "options")

        # Extract the tag property and figure out any necessary intersections, etc.
        #
        # Technically, a `const` value in JSON Schema can be an array or object, too... but like, we
        # only ever use `const` for describing enum variants and what not, so this is my middle-ground
        # approach to also allow for other constant-y types, but not objects/arrays which would
        # just... not make sense.
        tag_subschema = resolved_subschema.delete(enum_tag_field)
        tag_value = nil
    
        for allowed_type in ["string", "number", "boolean"] do
          maybe_tag_value = tag_subschema.dig("type", allowed_type, "const")
          if !maybe_tag_value.nil?
            tag_value = maybe_tag_value
            break
          end
        end

        if tag_value.nil?
          puts "All enum subschemas representing an internally-tagged enum must have the tag field use a const value."
          puts "Tag subschema: #{tag_subschema}"
          exit
        end

        if unique_tag_values.has_key?(tag_value)
          puts "Found duplicate tag value '#{tag_value}' when resolving enum subschemas."
          exit
        end

        unique_tag_values[tag_value] = tag_subschema

        # Now merge all of the properties from the given subschema, so long as the overlapping
        # properties have the same schema.
        resolved_subschema.each { |property, property_schema|
          resolved_property = unique_resolved_properties[property]
          if !resolved_property.nil?
            # The property is already being tracked, so just do a check to make sure the property from our
            # current subschema matches the existing property, schema-wise, before we update it.
            reduced_existing_property = get_reduced_schema(resolved_property["_resolved"])
            reduced_new_property = get_reduced_schema(property_schema)

            if reduced_existing_property != reduced_new_property
              puts "Had overlapping property '#{property}' from resolved enum subschema, but schemas differed:"
              puts "Existing property schema (reduced): #{reduced_existing_property}"
              puts "New property schema (reduced): #{reduced_new_property}"
              exit
            end

            # The schemas match, so just update the list of "relevant when" values.
            resolved_property["relevant_when"].push(tag_value)
          else
            # First time seeing this particular property, so insert it.
            resolved_property = { "_resolved" => property_schema, "relevant_when" => [tag_value] }
          end

          unique_resolved_properties[property] = resolved_property
        }
      }

      # Now that we've gone through all of the non-tag field, possibly overlapped properties, go
      # through and modify the properties so that we only keep the "relevant when" values if the
      # list of those values does not match the full set of unique tag values. We don't want to show
      # "relevant when" for fields that all variants share, basically.
      unique_tags = unique_tag_values.keys

      unique_resolved_properties.transform_values! { |value|
        # We check if a given property is relevant to all tag values by getting an intersection
        # between `relevant_when` and the list of unique tag values, as well as asserting that the
        # list lengths are identical.
        relevant_when = value["relevant_when"]
        if relevant_when.length == unique_tags.length && relevant_when & unique_tags == unique_tags
          value.delete("relevant_when")
        end

        value
      }

      # Now we build our property for the tag field itself, and add that in before returning all of
      # the unique resolved properties.
      tag_property = {
        "type" => {
          "string" => {
            "enum" => unique_tag_values.transform_values { |schema| get_rendered_description_from_schema(schema) },
          }
        }
      }

      unique_resolved_properties[enum_tag_field] = tag_property

      return { "_resolved" => { "type" => "object", "options" => unique_resolved_properties } }
    end
  end

  # Schema pattern: simple externally tagged enum with only unit variants.
  #
  # This a common pattern where basic enums that only have unit variants -- i.e. `enum { A, B, C }`
  # -- end up being represented by a bunch of subschemas that are purely `const` values.
  if enum_tagging == "external"
    tag_values = {}

    subschemas.each { |subschema|
      # For each subschema, try and grab the value of the `const` property and use it as the key for
      # storing this subschema.
      #
      # We take advantage of missing key index gets returning `nil` by checking below to make sure
      # none of the keys are nil. If any of them _are_ nill, then we know not all variants had a
      # `const` schema.
      tag_values[subschema["const"]] = subschema
    }

    if tag_values.keys.all? { |tag| !tag.nil? && tag.is_a?(String) }
      return { "_resolved" => { "type" => { "string" => {
        "enum" => tag_values.transform_values { |schema| get_rendered_description_from_schema(schema) },
      } } } }
    end
  end

  # Fallback schema pattern: mixed-use enums.
  #
  # These are enums that can basically be some combination of possible values: `Concurrency` is the
  # canonical example as it can be set via `"none"`, `"adaptive"`, or an integer between 1 and...
  # 2^64 or something like that. None of the subschemas overlap in any way.
  #
  # We just end up emitting a composite type output to cover each possibility, so the above would
  # have the `string` type with an `enum` of `"none"` and `"adaptive"`, and the uint type for the
  # integer side. This code mostly assumes the upstream schema is itself correct, in terms of not
  # providing a schema that is too ambiguous to properly validate against an input document.
  type_defs = {}
  type_modes = {}

  type_defs = subschemas.filter_map { |subschema| resolve_schema(root_schema, subschema) }
    .reduce { |acc, item| nested_merge(acc, item) }

  { "_resolved" => { "type" => type_defs }, "annotations" => "mixed_use" }
end

def get_rendered_description_from_schema(schema)
  # Grab both the raw description and raw title, and if the title is empty, just use the
  # description, otherwise concatenate the title and description with newlines so that there's a
  # whitespace break between the title and description.
  raw_description = schema.fetch("description", "")
  raw_title = schema.fetch("title", "")

  description = if raw_title.empty? then raw_description else "#{raw_title}\n\n#{raw_description}" end

  # Do some basic trimming to drop trailing whitespace, etc.
  description.strip!

  # TODO: We need to take the concatenated description and run it through a Markdown parser so that
  # we can actually render any links that are present, returning immediately usable output with
  # proper links, and so on.

  description
end

if ARGV.length < 3
  puts "usage: extract-component-schema.rb <configuration schema path> <component type> <component name>"
  exit
end

schema_path = ARGV[0]
component_type = ARGV[1]
component_name = ARGV[2]

schema_file = File.open schema_path
root_schema = JSON.load schema_file

component_types = ["source", "transform", "sink"]

# First off, we generate the component type configuration bases. These are the high-level
# configuration settings that are universal on a per-component type basis.
#
# For example, the "base" configuration for a sink would be the inputs, buffer settings, healthcheck
# settings, and proxy settings... and then the configuration for a sink would be those, plus
# whatever the sink itself defines.
component_bases = root_schema["definitions"].filter_map { |key, definition|
  config_docs_base_type = definition.dig("_metadata", "config_docs_base_type")
  {config_docs_base_type => definition} if component_types.include? config_docs_base_type
}
.reduce(:+)

component_bases.each { |component_type, schema|
  puts " <!> Resolving base definition for #{component_type}..."
  base_config = resolve_schema(root_schema, schema)
  final = { "base" => { "components" => { component_type => { "configuration" => base_config } } } }

  puts "base config for #{component_type}:"
  puts JSON.pretty_generate final
  puts ""
}
