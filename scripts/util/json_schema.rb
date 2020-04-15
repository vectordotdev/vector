require "json"
require "json_schemer"

class JSONSchema
  class << self
    def validate(schema_path, hash)
      schema = load_schema!(schema_path)
      errors = validate_schema(schema, hash)

      errors.collect do |error|
        "The value at `#{error.fetch("data_pointer")}` failed validation for `#{error.fetch("schema_pointer")}`, reason: `#{error.fetch("type")}`"
      end
    end

    private
      def load_schema!(schema_path)
        body = File.read("#{ROOT_DIR}#{schema_path}")
        JSON.parse(body)
      end

      def validate_schema(schema, hash)
        schemer = JSONSchemer.schema(schema, ref_resolver: 'net/http')
        schemer.validate(hash).to_a
      end
  end
end
