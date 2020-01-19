class Templates
  class ConfigWriter
    class Writer
      def category(name)
        raise NotImplementedError.new()
      end
    end

    class TOMLWriter < Writer
      PATH_DELIMITER = ".".freeze

      def initialize
        @indent = 0
        @string = ""
      end

      def category(name)
        last_line = @string.split("\n").last

        if last_line && (last_line[0] != "[" || last_line[-1] != "]")
          puts()
        end

        puts("# #{name}")
      end

      def hash(hash, path: [], tags: [])
        if hash.length > 1
          raise ArgumentError.new("A hash must contain only a single key and value")
        end

        key = nil
        value = nil

        if hash.values.first.is_a?(Hash)
          hash = hash.flatten
          key = hash.keys.first
          value = hash.values.first
        elsif hash.keys.first.include?(".")
          key = hash.keys.first.inspect
          value = hash.values.first
        else
          key = hash.keys.first
          value = hash.values.first
        end

        kv(key, value, path: path, tags: tags)
      end

      def indent(spaces)
        @indent += spaces
      end

      def kv(key, value, path: [], tags: [])
        quoted_key = key.include?(" ") ? key.to_toml : key
        full_key = (path + [quoted_key]).join(PATH_DELIMITER)
        line = "#{full_key} = #{value.to_toml}"

        if tags.any?
          line << " # #{tags.join(", ")}"
        end

        puts(line)
      end

      def print(string)
        @string << string
      end

      def puts(string = nil)
        if string == nil
          @string << "\n"
        else
          @string << ("#{string}".indent(@indent) + "\n")
        end
      end

      def table(name, array: false, path: [])
        full_name = (path + [name]).join(PATH_DELIMITER)

        if array
          puts("[[#{full_name}]]")
        else
          puts("[#{full_name}]")
        end

        indent(2)
      end

      def to_s
        @string.rstrip
      end
    end

    attr_reader :array, :block, :fields, :key_path, :table_path

    def initialize(fields, array: false, key_path: [], table_path: [], &block)
      if !fields.is_a?(Array)
        raise ArgumentError.new("fields must be an array")
      end

      if block_given?
        fields = fields.select(&block)
      end

      @array = array
      @fields = fields.sort_by(&:config_file_sort_token)
      @key_path = key_path
      @table_path = table_path
      @block = block
    end

    def categories
      @categories ||= fields.collect(&:category).uniq
    end

    def to_toml(table_style: :normal)
      raise NotImplementedError.new()
    end

    private
      def build_child_writer(fields, array: false, key_path: [], table_path: [])
        self.class.new(fields, array: array, key_path: key_path, table_path: table_path, &@block)
      end

      def field_tags(field, default: true, enum: true, example: false, optionality: true, relevant_when: true, type: true, short: false, unit: true)
        tags = []

        if optionality
          if field.required?
            tags << "required"
          else
            tags << "optional"
          end
        end

        if example
          if field.default.nil? && (!field.enum || field.enum.keys.length > 1)
            tags << "example"
          end
        end

        if default
          if !field.default.nil?
            if short
              tags << "default"
            else
              tags << "default: #{field.default.inspect}"
            end
          elsif field.optional?
            tags << "no default"
          end
        end

        if type
          if short
            tags << field.type
          else
            tags << "type: #{field.type}"
          end
        end

        if unit && !field.unit.nil?
          if short
            tags << field.unit
          else
            tags << "unit: #{field.unit}"
          end
        end

        if enum && field.enum
          if short && field.enum.keys.length > 1
            tags << "enum"
          else
            escaped_values = field.enum.keys.collect { |enum| enum.to_toml }
            if escaped_values.length > 1
              tags << "enum: #{escaped_values.to_sentence(two_words_connector: " or ")}"
            else
              tag = "must be: #{escaped_values.first}"
              if field.optional?
                tag << " (if supplied)"
              end
              tags << tag
            end
          end
        end

        if relevant_when && field.relevant_when
          word = field.required? ? "required" : "relevant"
          tag = "#{word} when #{field.relevant_when_kvs.to_sentence(two_words_connector: " or ")}"
          tags << tag
        end

        tags
      end

      def full_path
        table_path + key_path
      end
  end
end
