#!/usr/bin/env ruby

# load-meta.rb
#
# SUMMARY
#
#   A script that processes all files in the /.meta directory and echos the
#   results. This is necessary because the /.meta direction uses ERB templates,
#   making it easy to share common TOML snippets.
#
#   TODO: We should consider switching to YAML, JSONNET, or another format that
#   allows for reuse.


begin
  require "date"
  require "erb"
  require "json"
  require "toml-rb"
rescue LoadError => ex
  puts "Load error: #{ex.message}"
  exit
end


META_ROOT = "#{Dir.pwd}/.meta"

class Object
  def is_primitive_type?
    is_a?(String) ||
      is_a?(Integer) ||
      is_a?(TrueClass) ||
      is_a?(FalseClass) ||
      is_a?(NilClass) ||
      is_a?(Float)
  end

  def to_toml(hash_style: :expanded)
    if is_a?(Hash)
      values =
        (hash_style == :flatten ? flatten : self).
          select { |_k, v| !v.nil? }.
          collect do |k, v|
            "#{quote_toml_key(k)} = #{v.to_toml(hash_style: :inline)}"
          end

      if hash_style == :inline
        "{#{values.join(", ")}}"
      else
        values.join("\n")
      end
    elsif is_a?(Array)
      values = select { |v| !v.nil? }.collect { |v| v.to_toml(hash_style: :inline) }
      if any? { |v| v.is_a?(Hash) }
        "[\n" + values.join(",\n") + "\n]"
      else
        "[" + values.join(", ") + "]"
      end
    elsif is_a?(Date)
      iso8601()
    elsif is_a?(Time)
      strftime('%Y-%m-%dT%H:%M:%SZ')
    elsif is_a?(String) && include?("\n")
      result =
        <<~EOF
        """
        #{self}
        """
        EOF

      result.chomp
    elsif is_primitive_type?
      inspect
    else
      raise "Unknown value type: #{self.class}"
    end
  end

  private
    def quote_toml_key(key)
      if key.include?(".")
        "\"#{key}\""
      else
        "#{key}"
      end
    end
end


def render(path, args = {})
  context = binding

  args.each do |key, value|
    context.local_variable_set("#{key}", value)
  end

  full_path = path.start_with?("/") ? path : "#{META_ROOT}/#{path}"

  if !File.exists?(full_path) && File.exists?("#{full_path}.erb")
    full_path = "#{full_path}.erb"
  end

  body = File.read(full_path)
  renderer = ERB.new(body, nil, '-')

  renderer.result(context)
end

def load
  metadata = {}

  contents =
    Dir.glob("#{META_ROOT}/**/[^_]*.{toml,toml.erb}").
      sort.
      unshift("#{META_ROOT}/root.toml"). # move to the front
      uniq.
      collect do |file|
        begin
          render(file)
        rescue Exception => e
          raise(
            <<~EOF
            The follow metadata file failed to load:

              #{file}

            The error received was:

              #{e.message}
              #{e.backtrace.join("\n  ")}
            EOF
          )
        end
      end

  content = contents.join("\n")
  TomlRB.parse(content).to_json
end

puts load()
