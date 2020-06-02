require_relative "base"

module ConfigWriters
  class ExampleWriter < Base
    def to_toml(table_style: :normal)
      writer = TOMLWriter.new()

      if full_path.any? && table_style == :normal
        writer.table(full_path, array: array)
      end

      sorted_categories = fields.collect(&:category).uniq

      sorted_categories.each do |category|
        category_fields = fields.select { |field| field.category == category }

        if sorted_categories.size > 1
          writer.category(category)
        end

        category_fields.each do |field|
          examples = field.examples.fetch_group_values!(group)

          if field.children? && field.examples.empty?
            field_table_style = (field.toml_display || :inline).to_sym
            child_table_path = field_table_style == :normal ? (full_path + [field.name]) : table_path
            child_key_path = field_table_style == :normal ? [] : (key_path + [field.name])
            child_values = @values[field.name.to_sym]
            child_writer = build_child_writer(field.children_list, array: field.array?, group: group, key_path: child_key_path, table_path: child_table_path, values: child_values)
            toml = child_writer.to_toml(table_style: field_table_style)

            if toml != ""
              writer.puts(toml)
            end
          elsif field.wildcard?
            examples.each do |example|
              writer.hash(example, path: key_path, tags: ["example"])
            end
          else
            value = @values[field.name.to_sym] || field.default || examples.first
            tags = field_tags(field, enum: false, example: false, optionality: true, short: true, type: false)
            writer.kv(field.name, value, path: key_path, tags: tags)
          end
        end
      end

      writer.to_s
    end
  end
end
