#!/usr/bin/env ruby

begin
  require "json"
rescue LoadError => ex
  puts "Load error: #{ex.message}"
  exit
end

# Gets the JSON Schema-compatible type name for the given Ruby Value.
def type_as_string(value)
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

# Gets whether or not the given schema is a composite schema.
#
# Composite schemas are represented by `allOf` in JSON Schema, where the validation step of a
# composite schema is in fact ensuring that all subschemas validate against the given document.
def is_composite_schema(schema)
  !schema["allOf"].nil?
end

# Gets whether or not the given schema is an enum schema.
#
# Enum schemas are represented by `oneOf` in JSON Schema, where the validation step of an
# enum schema is ensuring that at least one of the subschemas validates against the given document.
def is_enum_schema(schema)
  !schema["oneOf"].nil?
end

# Gets the schema type for the given schema.
#
# The schema type (i.e. the `type` property in a schema) must either being a string, or an array
# with a single string element. Otherwise, `nil` is returned.
def get_schema_type(schema)
  schema_type = schema["type"]
  if schema_type.is_a?(String)
    return schema_type
  elsif schema_type.is_a?(Array) && schema_type.length == 1 && schema_type[0].is_a?(String)
    schema_type[0]
  end
end

# Gets whether or not the given schema is a "single value" schema.
#
# A single value schema is any schema that maps to a single value, whether it's a "const" or a
# string, number, or boolean. Arrays and objects do not count, and composite/enum schemas are also
# not considered single value schemas, even if they only contain a single subschema which itself is
# a single value schema.
def is_single_value_schema(root_schema, schema)
  # Resolve any schema reference first.
  schema = resolve_schema_reference(root_schema, schema)

  # Cannot be a composite or enum schema.
  if !schema["allOf"].nil? || !schema["oneOf"].nil?
    return false
  end

  # If the type of the schema is a single value that is _not_ `array` or `object`, or `const` is
  # specified and also is not an array or object, or `enum` where each value is a single value, with
  #identi then
  # we have a single value schema.

  # Our primary check, where we assert that one of the following is true:
  # - `type` is a string, and the value is present in `allowed_types`
  # - `type` is an array, with a single element, whose value is present in `allowed_types`
  # - `const` is present, and neither an array nor hash
  # - `enum` is present, is an array, has no values that are an array or hash, and has elements that
  #   are all of the same type (i.e. all strings, or all numbers)
  allowed_types = ["string", "number", "boolean", "integer"]
  schema_type = get_schema_type(schema)
  const_value = schema["const"]
  enum_value = schema["enum"]
  allowed_types.include?(schema_type) ||
    allowed_types.include?(type_as_string(const_value)) ||
    (enum_value.is_a?(Array) && enum_value.all? { |evalue|
      allowed_types.include?(type_as_string(evalue)) && type_as_string(evalue) == type_as_string(enum_value[0])
    })
end

# Gets a schema definition from the root schema, by name.
def get_schema_by_name(root, schema_name)
  schema_name = schema_name.gsub(/#\/definitions\//, "")
  schema_def = root.dig("definitions", schema_name)
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
  puts "Entered `resolve_schema_reference`."

  if !schema["$ref"].nil?
    puts "  Schema reference detected ('#{schema["$ref"]}'), resolving..."

    resolved_schema = get_schema_by_name(root, schema["$ref"])

    current_schema = schema.clone
    current_schema.delete("$ref")
    schema = resolved_schema.merge(current_schema)
  end

  schema
end

def resolve_schema_property_type(root_schema, property)
  puts "Entered `resolve_schema_property_type`."

  property = resolve_schema_reference(root_schema, property)

  # Like `resolve_schema`, we might be asked to handle a composite/enum schema for an individual
  # property on an object, so if we detect that, we need to hairpin that back to `resolve_schema` to
  # handle it in a uniform fashion.
  if is_composite_schema(property) || is_enum_schema(property)
    puts "  Schema provided for property type resolution is composite/enum, resolving that first..."
    property = resolve_schema(root_schema, property)
    puts "  Finished resolving composite/enum schema, resuming property type resolution."
  end

  case property["type"]
  when "array"
    { "array" => { "items" => { "type" => resolve_schema_property_type(root_schema, property["items"]) } } }
  when "object"
    # This is perhaps a little weird, but we hairpin back to `resolve_schema`.
    #
    # When we get here, it's because we're processing a property of an object... where the property
    # itself will also inherently be a schema. It might be a simple scalar schema, or a schema
    # reference, or whatever... but it's a schema, and so we need to fully resolve it first.
    #
    # Based on the format that the output data needs to be in, however, we can't just directly nest
    # the resolved schema for an object within the property that uses it, but we instead must add
    # this little indirection layer of `"options": { ... }` first.
    { "object" => { "options" => resolve_schema(root_schema, property) } }
  when "string"
    string_def = {}

    string_def["default"] = property["default"] if property["default"].is_a? String

    { "string" => string_def }
  when "number"
    number_def = {}

    number_def["default"] = property["default"] if property["default"].is_a? Numeric

    { "number" => number_def }
  when "boolean"
    bool_def = {}

    bool_def["default"] = property["default"] if [true, false].include? property["default"]

    { "bool" => bool_def }
  else
    # We might be dealing with a const schema, where a constant value is the only valid match.
    const_value = property["const"]
    if !const_value.nil?
      return { "const" => const_value }
    end

    # We might be dealing with a "resolved" schema, which is where we wrap another layer around a
    # schema so that we can bundle annotations/metadata next to it for allowing downstream code to
    # additionally modify the resulting output.
    #
    # If we are, then handle it, otherwise... yell loudly.
    if property.has_key?("_resolved")
      puts "  Property schema was resolved with annotations, unwrapping..."

      # "Resolved" schemas should already be in the shape that we would otherwise be returning here,
      # so we actually just need to unnest them by a single level to maintain that.

      # TODO: Where should be dealing with annotations? For the buffer type stuff, we would need to
      # be pushing that back up to `resolve_schema` so that it could adjust the resulting
      # description, etc.

      unwrapped = property.dig("_resolved", "type")
      #puts "    Unwrapped: #{unwrapped}"
      return unwrapped
    else
      puts "Unknown type '#{property["type"]}' in configuration schema:"
      puts "Schema: #{property}"
      exit
    end
  end
end

# Fully resolves the schema.
#
# This recursively resolves schema references, as well as flattening them into a single object, and
# transforming certain usages -- composite/enum (`allOf`, `oneOf`), etc -- into more human-readable
# forms.
def resolve_schema(root_schema, schema)
  puts "Entered `resolve_schema`."

  # If this schema is referencing another schema definition, resolve that reference first, which
  # will essentially hydrate that reference by merging it into the current schema.
  schema = resolve_schema_reference(root_schema, schema)

  resolved_properties = {}

  # Figure out if we're looking at a composite schema (`allOf`), an enum schema (`oneOf`), or a
  # plain schema.
  if is_composite_schema(schema)
    # Composite schemas are simple, since we just add all of their resolved properties together.
    schema["allOf"].each { |subschema|
      resolved_subproperties = resolve_schema(root_schema, subschema)
      resolved_properties.merge!(resolved_subproperties)
    }
  elsif is_enum_schema(schema)
    # We completely defer resolution of enum schemas to `resolve_enum_subschemas` because there's a
    # lot of tricks and matching we need to do to suss out patterns that can be represented in more
    # condensed resolved forms.
    resolved_subproperties = resolve_enum_subschemas(root_schema, schema)
    puts "  Resolved following properties from enum subschemas: #{resolved_subproperties["_resolved"].keys}"
    resolved_properties.merge!(resolved_subproperties)
  else
    # We're dealing with a "plain" schema, which would be any of the normal types such as `string`,
    # `number`, `object`, and so on.

    # First, skip any field that is marked to be skipped. We use this, primarily, to influence
    # resolution for split definitions such as components. A sink's configuration is the sum of the
    # `SinkOuter` and the sink's specific enum variant, so we need to resolve them separately and
    # then rejoin them later on.
    if schema.dig("_metadata", "config_docs_skip")
      return {}
    end

    # Based on how we resolve schemas, we should never encounter a pure scalar schema here (string,
    # number, etc) expect as a property on an object schema.
    if schema["type"] != "object"
      puts "Cannot resolve schema definitions that are not composite, enum, or object-based schemas."
      exit
    end

    required = schema["required"] || []

    puts "  Schema: #{schema}"
    puts "  Resolving object schema with following properties: #{schema["properties"].keys}"

    schema["properties"].each { |property_name, property|
      puts "  Resolving property '#{property_name}'..."
      puts "    Property schema: #{property}"

      # Almost all of the magic occurs in `resolve_schema_property_type` where we're handling the
      # property type. For scalars, it's essentially a passthrough, but for arrays and objects, we
      # specifically handle those as they're nested _slightly_ different.
      resolved_property = {
        "type" => resolve_schema_property_type(root_schema, property),
        "required" => required.include?(property_name),
      }

      if !property["title"].nil?
        resolved_property["title"] = property["title"]
      end

      if !property["description"].nil?
        resolved_property["description"] = property["description"]
      end

      resolved_properties[property_name] = resolved_property

      puts "  Resolved property '#{property_name}'."
    }
  end

  resolved_properties
end

# Gets a reduced version of a schema.
#
# The reduced version strips out extraneous fields from the given schema, such that a value should
# be returned that is suitable for comparison with other schemas, to determine if the schemas --
# specifically the values that are allowed/valid -- are the same, while ignoring things like titles
# and descriptions.
def get_reduced_schema(schema)
  disallowed_properties = ["title", "description", "_metadata"]
  schema.delete_if { |key, value| disallowed_properties.include?(key) }

  if is_composite_schema(schema)
    schema["allOf"].map!(get_reduced_schema)
  end

  if is_enum_schema(schema)
    schema["oneOf"].map!(get_reduced_schema)
  end

  schema
end

def resolve_enum_subschemas(root_schema, schema)
  puts "Entered `resolve_enum_subschemas`."

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
      puts "  Trying to resolve enum subschemas as X-or-array-of-X..."

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
    puts "  Trying to resolve enum subschemas as simple internally-tagged enum..."

    unique_resolved_properties = {}
    unique_tag_values = {}

    subschemas.each { |subschema|
      puts "    Resolving internally-tagged enum subschema..."

      # For each subschema, resolve all of its properties.
      resolved_subschema = resolve_schema(root_schema, subschema)

      # Extract the tag property and figure out any necessary intersections, etc.
      tag_subschema = resolved_subschema.delete(enum_tag_field)
      tag_value = tag_subschema.dig("type", "const")
      if tag_value.nil?
        puts "All enum subschemas representing an internally-tagged enum must have the tag field use a const value."
        puts "Tag subschema: #{tag_subschema}"
        exit
      end

      if unique_tag_values.has_key?(tag_value)
        puts "Found duplicate tag value '#{tag_value}' when resolving enum subschemas."
        exit
      end

      puts "      Resolved subschema for tag value '#{tag_value}' has following properties: #{resolved_subschema.keys}"

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

puts "relevant_when: #{resolved_property}"

          # The schemas match, so just update the list of "relevant when" values.
          resolved_property["relevant_when"].push(tag_value)
        else
          puts "First time seeing property '#{property}' during enum subschema resolution."
          # First time seeing this particular property, so insert it.
          resolved_property = { "_resolved" => property_schema, "relevant_when" => [tag_value] }
        end

        unique_resolved_properties[property] = resolved_property
      }

puts "    Resolved internally-tagged enum subschema for tag value '#{tag_value}'."
puts "      Unique properties after last resolved enum subschema: #{unique_resolved_properties.keys}"
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

    return { "_resolved" => unique_resolved_properties }
  end

  # Schema pattern: simple externally tagged enum with only unit variants.
  #
  # This a common pattern where basic enums that only have unit variants -- i.e. `enum { A, B, C }`
  # -- end up being represented by a bunch of subschemas that are purely `const` values.
  if enum_tagging == "external"
    puts "  Trying to resolve enum subschemas as simple externally-tagged unit variants-only enum..."

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

  # Schema pattern: mixed-use single value enums.
  #
  # These are enums that can basically be some combination of possible values: `Concurrency` is the
  # canonical example as it can be set via `"none"`, `"adaptive"`, or an integer between 1 and...
  # 2^64 or something like that. None of the subschemas overlap in any way.
  #
  # We just end up emitting a composite type output to cover each possibility, so the above would
  # have the `string` type with an `enum` of `"none"` and `"adaptive"`, and the uint type for the
  # integer side.
  #
  # TODO: We don't handle every single possible combination flawlessly here. For example, a
  # subschema that could be a generic string _or_ a specific const value can't be trivially
  # specified, because once we think we're dealing with a bunch of possible const string values,
  # etc... we're sort of locked in.
  if enum_tagging == "external"
    puts "  Trying to resolve enum subschemas as mixed-use single value enum..."

    # See if all of the subschemas are "single value" schemas.
    if subschemas.all? { |subschema| is_single_value_schema(root_schema, subschema) }
      puts "Enum subschemas are all 'single value' schemas."

      # Now we need to group these subschemas into their respective type buckets, with some
      # constraints. We track both the per-type definitions, as well as what "mode" each type is,
      # so that we can detect incompatibilites, such as if we've already seen a bunch of subschemas
      # with const values that are, say, numbers... and then we hit another subschema that accepts
      # any generic number.
      #
      # We don't want to allow clashes like that because they're hard to generate sensible docs
      # output for, and they might just be plain non-sensicial in terms of actual deserialization.
      types = {}
      type_modes = {}

      subschemas.each { |subschema|
        if !subschema["const"].nil? || !subschema["enum"].nil?
          # We're dealing with a const/enum subschema, so get the _type_ of the value of `const` or
          # `enum`, which will inform us what data type we start an "enum" definition for.
          enum_values = subschema["enum"] || [subschema["const"]]
          enum_value_type = type_as_string(enum_values[0])

          # Make sure we're not clashing in terms of mode (enum vs any).
          existing_type_mode = type_modes[enum_value_type]
          if !existing_type_mode.nil? && existing_type_mode != "enum"
            puts "Already tracking a type definition for '#{enum_value_type}' of '#{existing_type_mode}', but tried to add 'enum'."
            exit
          else
            type_modes[enum_value_type] = "enum"
          end

          # Merge in the enum values we just got with whatever is already there for the type
          # definition, creating the type definition if it doesn't yet exist.
          existing_type_def = types.fetch(enum_value_type, { "enum" => [] })
          existing_type_def["enum"] << enum_values
          types[enum_value_type] = existing_type_def
        else
          # We're dealing with a "any valid value" subschema, so like the const/enum codepath, grab
          # the value type, make sure we aren't clashing, etc etc.
          any_value_type = get_schema_type(subschema)

          # Make sure we're not clashing in terms of mode (enum vs any), but also, make sure nothing
          # else already claimed this type definition, since also having duplicate subschemas for
          # the same type wouldn't make any sense... and is probably indicative of invalid logic in
          # the configuration schema codegen.
          existing_type_mode = type_modes[any_value_type]
          if !existing_type_mode.nil?
            puts "Already tracking a type definition for '#{any_value_type}' of '#{existing_type_mode}', but tried to add 'any'."
            exit
          else
            # TODO: This is where we would add things like validation constraints, etc, to the type definition.
            type_modes[any_value_type] = "any"
            types[any_value_type] = {}
          end
        end
      }

      return { "_resolved" => { "type" => types , "annotations" => "mixed_use" } }
    end
  end

  puts "Failed to resolve enum subschemas as anmy known enum pattern."
  exit
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
  base_config = resolve_schema(root_schema, schema)
  final = { "base" => { "components" => { component_type => { "configuration" => base_config } } } }

  puts "base config for #{component_type}:"
  puts JSON.pretty_generate final
  puts ""
}
