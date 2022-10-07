#!/usr/bin/env ruby

begin
  require "json"
  require "tempfile"
rescue LoadError => ex
  puts "Load error: #{ex.message}"
  exit
end

@log_nested_level = 0
@logging_enabled = false

def enter_level()
  @log_nested_level = @log_nested_level + 1
end

def exit_level()
  @log_nested_level = @log_nested_level - 1
end

def nested_log(msg)
  if !@logging_enabled
    return
  end

  level = [@log_nested_level - 1, 1].max
  indent = "  " * level
  puts "#{level}#{indent}#{msg}"
end

def deep_copy(obj)
  Marshal.load(Marshal.dump(obj))
end

def is_mergeable(value)
  value.is_a?(Hash) || value.is_a?(Array)
end

def nested_merge(base, override)
  # Handle some basic cases first.
  if base.nil?
    return override
  elsif override.nil?
    return base
  elsif !is_mergeable(base) && !is_mergeable(override)
    return override
  end

  merger = proc { |_, v1, v2| Hash === v1 && Hash === v2 ? v1.merge(v2, &merger) : Array === v1 && Array === v2 ? v1 | v2 : [:undefined, nil, :nil].include?(v2) ? v1 : v2 }
  base.merge(override.to_h, &merger)
end

def sort_hash_nested(input)
  input.keys.sort().reduce({}) { |acc, key|
    acc[key] = if input[key].is_a?(Hash)
      sort_hash_nested(input[key])
    else
      input[key]
    end
    acc
  }
end

def write_to_temp_file(prefix, data)
  file = Tempfile.new(prefix)
  file.write(data)
  file.close()

  file.path
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
  value_type = type_str(value)
  case value_type
  when "number"
    ["uint", "int", "float"].each { |numeric_type|
      type_field = schema.dig("type", numeric_type)
      if !type_field.nil?
        return type_field
      end
    }
  else
    schema.dig("type", value_type)
  end
end

def format_relevant_when(field, values)
  values.map { |value| "#{field} = #{JSON.dump(value)}" }
    .to_a
    .join(" or ")
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
    nested_log "Could not find schema definition '#{schema_name}' in given schema."
    exit
  end

  schema_def
end

# Applies various fields to an object property.
#
# This includes items such as any default value that is present, or whether or not the property is
# required.
def apply_object_property_fields!(parent_schema, property_schema, property_name, property)
  nested_log "Applying object property fields for '#{property_name}'..."

  # Since defaults apply in a top-down fashion -- i.e. a self-defined default value for type A would
  # be overridden by a default set specifically on a field of type A -- we have to check both the
  # parent schema and property schema itself.
  #
  # If we end using the parent schema's default, we have to make sure we merge it appropriately. For
  # scalars, this is simple, but for objects, we want to each the default value of each property in
  # the object individually, etc.
  parent_default_value = parent_schema.dig("default", property_name)
  property_default_value = property_schema["default"]
  default_value = nested_merge(property_default_value, parent_default_value)

  required_properties = parent_schema["required"] || []

  # Set the default value, if one is present.
  if !default_value.nil?
    nested_log "Property has default value: #{default_value}"

    property_default_type = type_str(default_value)
    property_type_field = get_schema_type_field_for_value(property, default_value)
    if !property_type_field.nil?
      # For objects, we apply default property values in a semi-nested fashion, since we don't want
      # to show a default level for the object field itself, and instead want to show the
      # defaults for each of the properties within _this_ object property.
      #
      # Otherwise, set it directly, since we're dealing with a scalar.
      if property_default_type == "object"
        default_value.each { |key, value|
          subproperty = property.dig("type", "object", "options", key)
          if !subproperty.nil?
            subproperty_type_field = get_schema_type_field_for_value(subproperty, value)
            if !subproperty_type_field.nil?
              subproperty_type_field["default"] = value
            end
          end
        }
      else
        property_type_field["default"] = default_value
      end
    end
  end

  # Set whether or not this property is required.
  property["required"] = required_properties.include?(property_name) && default_value.nil?
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

  while !schema["$ref"].nil?
    resolved_schema = get_schema_by_name(root, schema["$ref"])

    schema.delete("$ref")
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

  allowed_properties = ["type", "const", "enum", "allOf", "oneOf", "$ref", "items"]
  schema.delete_if { |key, value| !allowed_properties.include?(key) }

  if schema.has_key?("allOf")
    schema["allOf"].map!(get_reduced_schema)
  end

  if schema.has_key?("oneOf")
    schema["oneOf"].map!(get_reduced_schema)
  end

  schema
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
  if schema.dig("_metadata", "hidden")
    nested_log "Instructed to skip resolution for the given schema."
    return
  end

  enter_level()

  # If this schema references another schema definition, resolve that schema definition and merge
  # it back into our schema, flattening it out.
  schema = resolve_schema_reference(root_schema, schema)

  # Now simply resolve the schema, depending on what type it is.
  resolved = case get_schema_type(schema)
    when "all-of"
      nested_log "Resolving composite schema."

      # Composite schemas are indeed the sum of all of their parts, so resolve each subschema and
      # merge their resolved state together.
      reduced = schema["allOf"].filter_map { |subschema| resolve_schema(root_schema, subschema) }
        .reduce { |acc, item| nested_merge(acc, item) }
      reduced["type"]
    when "one-of"
      nested_log "Resolving enum schema."

      # We completely defer resolution of enum schemas to `resolve_enum_schema` because there's a
      # lot of tricks and matching we need to do to suss out patterns that can be represented in more
      # condensed resolved forms.
      wrapped = resolve_enum_schema(root_schema, schema)

      # `resolve_enum_schema` always hands back the resolved schema under the key `_resolved`, so
      # that we can add extra details about the resolved schema (anything that will help us render
      # it better in the docs output) on the side. That's why we have to unwrap the resolved schema
      # like this.

      # TODO: Do something with the extra detail (`annotations`).

      wrapped.dig("_resolved", "type")
    when "array"
      nested_log "Resolving array schema."

      { "array" => { "items" => resolve_schema(root_schema, schema["items"]) } }
    when "object"
      nested_log "Resolving object schema."

      # TODO: Not all objects have an actual set of properties, such as anything using
      # `additionalProperties` to allow for arbitrary key/values to be set, which is why we're
      # handling the case of nothing in `properties`... but we probably want to be able to better
      # handle expressing this in the output.. or maybe it doesn't matter, dunno!
      properties = schema["properties"] || {}

      options = properties.filter_map { |property_name, property_schema|
        nested_log "Resolving object property '#{property_name}'..."
        resolved_property = resolve_schema(root_schema, property_schema)
        if !resolved_property.nil?
          apply_object_property_fields!(schema, property_schema, property_name, resolved_property)

          nested_log "Resolved object property '#{property_name}'."

          [property_name, resolved_property]
        else
          nested_log "Resolution failed for '#{property_name}'."
        end
      }

      { "object" => { "options" => options.to_h } }
    when "string"
      nested_log "Resolving string schema."

      string_def = {}
      string_def["default"] = schema["default"] if !schema["default"].nil?
  
      { "string" => string_def }
    when "number"
      nested_log "Resolving number schema."

      numeric_type = schema.dig("_metadata", "numeric_type") || "number"
      number_def = {}
      number_def["default"] = schema["default"] if !schema["default"].nil?
  
      { numeric_type => number_def }
    when "boolean"
      nested_log "Resolving boolean schema."

      bool_def = {}
      bool_def["default"] = schema["default"] if !schema["default"].nil?
  
      { "bool" => bool_def }
    when "const"
      nested_log "Resolving const schema."

      # For `const` schemas, just figure out the type of the constant value so we can generate the
      # resolved output.
      const_value = schema["const"]
      const_type = type_str(const_value)
      { const_type => { "const" => const_value } }
    when "enum"
      nested_log "Resolving enum const schema."

      # Similarly to `const` schemas, `enum` schemas are merely multiple possible constant values. Given
      # that JSON Schema does allow for the constant values to differ in type, we group them all by
      # type to get the resolved output.
      enum_values = schema["enum"]
      grouped = enum_values.group_by { |value| type_str(value) }
      grouped.transform_values! { |values| { "enum" => values } }
      grouped
    else
      puts "Failed to resolve the schema. Schema: #{schema}"
      exit
    end

  exit_level()

  output = { "type" => resolved }
  description = get_rendered_description_from_schema(schema)
  output["description"] = description if !description.empty?
  output
end

def resolve_enum_schema(root_schema, schema)
  enter_level()

  subschemas = schema["oneOf"]
  subschema_count = subschemas.count

  # Collect all of the tagging mode information upfront.
  enum_tagging = schema.dig("_metadata", "enum_tagging")
  if enum_tagging.nil?
    puts "Enum schemas should never be missing the metadata for the enum tagging mode."
    puts "Schema: #{JSON.pretty_generate(schema)}"
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
      nested_log "Detected likely 'X or array of X' enum schema, applying further validation..."
  
      single_idx = if array_idx == 0 then 1 else 0 end

      # We 'reduce' the subschemas, which strips out all things that aren't fundamental, which boils
      # it down to only the shape of what the schema accepts, so no title or description or default
      # values, and so on.
      single_reduced_subschema = get_reduced_schema(subschemas[single_idx])
      array_reduced_subschema = get_reduced_schema(subschemas[array_idx])

      if single_reduced_subschema == array_reduced_subschema["items"]
        nested_log "Reduced schemas match, fully resolving schema for X..."

        # The single subschema and the subschema for the array items are a match! We'll resolve this
        # as the schema of the "single" option, but with an annotation that it can be specified
        # multiple times.

        # Copy the subschema, and if the parent schema we're resolving has a default value with a
        # type that matches the type of the "single" subschema, add it to the copy of that schema.
        #
        # It's hard to propagate the default from the configuration schema generation side, but much
        # easier to do so here.
        single_subschema = deep_copy(subschemas[single_idx])
        if get_schema_type(single_subschema) == type_str(schema["default"])
          single_subschema["default"] = schema["default"]
        end
        resolved_subschema = resolve_schema(root_schema, subschemas[single_idx])

        nested_log "Resolved as 'X or array of X' enum schema."
        #nested_log "Original schema: #{schema}"

        exit_level()

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
    nested_log "Resolving enum subschemas to detect 'object'-ness..."

    # This transformation only makes sense when all subschemas are objects, so only resolve the ones
    # that are objects, and only proceed if all of them were resolved.
    resolved_subschemas = subschemas.filter_map { |subschema|
      resolve_schema(root_schema, subschema) if get_schema_type(subschema) == "object"
    }

    if resolved_subschemas.count == subschemas.count
      nested_log "Detected likely 'internally-tagged with named fields' enum schema, applying further validation..."

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
          puts "Tag subschema: #{JSON.pretty_generate(tag_subschema)}"
          exit
        end

        if unique_tag_values.has_key?(tag_value)
          puts "Found duplicate tag value '#{tag_value}' when resolving enum subschemas."
          exit
        end

        unique_tag_values[tag_value] = tag_subschema

        # Now merge all of the properties from the given subschema, so long as the overlapping
        # properties have the same schema.
        resolved_subschema.each { |property_name, property_schema|
          existing_property = unique_resolved_properties[property_name]
          resolved_property = if !existing_property.nil?
            # The property is already being tracked, so just do a check to make sure the property from our
            # current subschema matches the existing property, schema-wise, before we update it.
            reduced_existing_property = get_reduced_schema(existing_property)
            reduced_new_property = get_reduced_schema(property_schema)

            if reduced_existing_property != reduced_new_property
              puts "Had overlapping property '#{property_name}' from resolved enum subschema, but schemas differed:"
              puts "Existing property schema (reduced): #{reduced_existing_property}"
              puts "New property schema (reduced): #{reduced_new_property}"
              exit
            end

            # The schemas match, so just update the list of "relevant when" values.
            existing_property["relevant_when"].push(tag_value)
            existing_property
          else
            # First time seeing this particular property.
            property_schema["relevant_when"] = [tag_value]
            property_schema
          end

          unique_resolved_properties[property_name] = resolved_property
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

        # Add enough information from consumers to figure out _which_ field needs to have the given
        # "relevant when" value.
        if value.has_key?("relevant_when")
          value["relevant_when"] = format_relevant_when(enum_tag_field, value["relevant_when"])
        end

        value
      }

      # Now we build our property for the tag field itself, and add that in before returning all of
      # the unique resolved properties.
      unique_resolved_properties[enum_tag_field] = {
        "type" => {
          "string" => {
            "enum" => unique_tag_values.transform_values { |schema| get_rendered_description_from_schema(schema) },
          }
        }
      }

      nested_log "Resolved as 'internally-tagged with named fields' enum schema."
      #nested_log "Original schema: #{schema}"

      exit_level()

      return { "_resolved" => { "type" => { "object" => { "options" => unique_resolved_properties } } } }
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
      nested_log "Resolved as 'externally-tagged with only unit varaints' enum schema."

      exit_level()

      return { "_resolved" => { "type" => { "string" => {
        "enum" => tag_values.transform_values { |schema| get_rendered_description_from_schema(schema) },
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

  nested_log "Resolved as 'fallback mixed-mode' enum schema."

  exit_level()

  { "_resolved" => { "type" => type_defs["type"] }, "annotations" => "mixed_mode" }
end

def get_rendered_description_from_schema(schema)
  # Grab both the raw description and raw title, and if the title is empty, just use the
  # description, otherwise concatenate the title and description with newlines so that there's a
  # whitespace break between the title and description.
  raw_description = schema.fetch("description", "")
  raw_title = schema.fetch("title", "")

  description = if raw_title.empty? then raw_description else "#{raw_title}\n\n#{raw_description}" end
  description.strip
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
.reduce(:merge)

component_bases.each { |component_type, schema|
  puts "[*] Resolving base definition for #{component_type}..."
  base_config = resolve_schema(root_schema, schema)

  unwrapped_base_config = base_config.dig("type", "object", "options")
  if unwrapped_base_config.nil?
    puts "Base configuration types must always resolve to an object schema."
    exit
  end

  unwrapped_base_config = sort_hash_nested(unwrapped_base_config)

  final = { "base" => { "components" => { component_type => { "configuration" => unwrapped_base_config } } } }

  final_json = JSON.pretty_generate(final)
  json_output_file = write_to_temp_file(["config-schema-base-#{component_type}-", ".json"], final_json)
  puts "[✓] Wrote base #{component_type} configuration to '#{json_output_file}'. (#{final_json.length} bytes)"

  puts "[*] Importing base #{component_type} configuration as Cue file..."
  cue_output_file = "#{component_type}_base.cue"
  if !system("cue", "import", "-f", "-o", cue_output_file, "-p", "metadata", json_output_file)
    puts "[!] Failed to import base #{component_type} configuration as Cue."
    exit
  end
  puts "[✓] Imported base #{component_type} configuration as Cue."
}
