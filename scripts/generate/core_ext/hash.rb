class Hash
  def delete!(key)
    delete(key) { |key| raise("Key does not exist: #{key.inspect}") }
  end
end