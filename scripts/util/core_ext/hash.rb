require 'ostruct'

class Hash
  def delete!(key)
    delete(key) { |key| raise("Key does not exist: #{key.inspect}") }
  end

  def flatten
    each_with_object({}) do |(k, v), h|
      if v.is_a? Hash
        v.flatten.map do |h_k, h_v|
          h["#{k}.#{h_k}"] = h_v
        end
      else
        h[k] = v
      end
    end
  end

  def to_query(*args)
    to_param(*args).gsub("[]", "").gsub("%5B%5D", "")
  end

  def to_struct(should_have_keys: [], &block)
    new_hash = {}

    each do |key, val|
      new_hash[key] =
        if val.is_a?(Hash) && (should_have_keys.empty? || (should_have_keys - val.keys).empty?)
          if block_given?
            yield(key, val)
          else
            val.to_struct(should_have_keys: should_have_keys)
          end
        elsif val.is_a?(Array)
          val.collect do |item|
            if item.is_a?(Hash)
              item.to_struct
            else
              item
            end
          end
        else
          val
        end
    end

    AccessibleHash.new(new_hash)
  end

  def to_struct_with_name(constructor: nil, ensure_keys: [], should_have_keys: [])
    to_struct(should_have_keys: should_have_keys) do |key, hash|
      new_hash = {}

      ensure_keys.each do |key|
        new_hash[key] = nil
      end

      new_hash.merge!(hash)
      new_hash["name"] = key

      if constructor
        constructor.new(new_hash)
      else
        new_hash.to_struct
      end
    end
  end


  def validate_schema
    schema_path = self["$schema"]

    if schema_path
      JSONSchema.validate(schema_path, self)
    else
      []
    end
  end
end
