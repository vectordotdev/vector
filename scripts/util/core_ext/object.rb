class Object
  def deep_to_h
    if is_a?(OpenStruct)
      to_h.sort.to_h.deep_to_h
    elsif is_a?(Hash)
      new_h = {}
      each do |k, v|
        new_h[k] = v.deep_to_h
      end
      new_h
    elsif is_a?(Array)
      map(&:deep_to_h)
    elsif respond_to?(:to_h)
      to_h.sort.to_h
    else
      self
    end
  end

  def is_primitive_type?
    is_a?(String) ||
      is_a?(Integer) ||
      is_a?(TrueClass) ||
      is_a?(FalseClass) ||
      is_a?(NilClass) ||
      is_a?(Float)
  end

  def to_toml
    if is_a?(Hash)
      values = select { |_k, v| !v.nil? }.collect { |k, v| if k.include? "."
        "\"#{k}\" = #{v.to_toml}"
      else
        "#{k} = #{v.to_toml}"
      end}
      values.join("\n")
    elsif is_a?(Array)
      values = select { |v| !v.nil? }.collect { |v| v.to_toml }
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
end
