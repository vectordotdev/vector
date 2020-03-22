class Platform
  attr_reader :archs,
    :interfaces,
    :name,
    :oss,
    :strategies,
    :title

  def initialize(hash)
    @archs = hash.fetch("archs")
    @interfaces = hash.fetch("interfaces")
    @name = hash.fetch("name")
    @oss = hash.fetch("oss")
    @strategies = hash.fetch("strategies").collect(&:to_struct)
    @title = hash.fetch("title")
  end

  def logo_path
    return @logo_path if defined?(@logo_path)
    path = "/img/logos/#{name}.svg"
    @logo_path = File.exists?("#{STATIC_ROOT}#{path}") ? path : nil
  end

  def to_h
    {
      archs: archs,
      interfaces: interfaces,
      name: name,
      oss: oss,
      strategies: strategies,
      title: title
    }
  end
end
