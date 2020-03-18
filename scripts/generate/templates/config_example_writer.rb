require_relative "config_writer"

class Templates
  class ConfigExampleWriter < ConfigWriter
    def to_toml(table_style: :normal)
      writer = TOMLWriter.new()

      if full_path.any? && table_style == :normal
        writer.table(full_path, array: array)
      end

      fields.group_by(&:category).each do |category, category_fields|
        if categories.size > 1
          writer.category(category)
        end

        category_fields.each do |field|
          if field.children?
            field_table_style = (field.toml_display || :inline).to_sym
            child_table_path = field_table_style == :normal ? (full_path + [field.name]) : table_path
            child_key_path = field_table_style == :normal ? [] : (key_path + [field.name])
            child_values = @values[field.name.to_sym]
            child_writer = build_child_writer(field.children_list, array: field.array?, key_path: child_key_path, table_path: child_table_path, values: child_values)
            toml = child_writer.to_toml(table_style: field_table_style)

            if toml != ""
              writer.puts(toml)
            end
          elsif field.wildcard?
            field.examples.each do |example|
              writer.hash(example, path: key_path, tags: ["example"])
            end
          else
            value = @values[field.name.to_sym] || field.default || field.examples.first
            tags = field_tags(field, enum: false, example: false, optionality: true, short: true, type: false)
            writer.kv(field.name, value, path: key_path, tags: tags)
          end
        end
      end

      writer.to_s
    end
  end
end
