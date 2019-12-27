class Hash
  def delete!(key)
    delete(key) { |key| raise("Key does not exist: #{key.inspect}") }
  end

  def to_query(*args)
    to_param(*args).gsub("[]", "").gsub("%5B%5D", "")
  end
end
