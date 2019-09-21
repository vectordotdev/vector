class Object
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
      values = select { |_k, v| !v.nil? }.collect { |k, v| "#{k} = #{v.to_toml}" }
      "{" + values.join(", ") + "}"
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
