class Installation
  attr_reader :downloads, :operating_systems, :package_managers, :platforms

  def initialize(hash)
    @downloads = hash.fetch("downloads").to_struct_with_name(ensure_keys: ["package_manager"])
    @operating_systems = hash.fetch("operating_systems").to_struct_with_name
    @package_managers = hash.fetch("package_managers").to_struct_with_name
    @platforms = hash.fetch("platforms").to_struct_with_name
  end

  def downloads_list
    @downloads_list ||= downloads.to_h.values.sort_by(&:name)
  end

  def operating_systems_list
    @operating_systems_list ||= operating_systems.to_h.values.sort_by(&:title)
  end

  def package_managers_list
    @package_managers_list ||= package_managers.to_h.values.sort_by(&:title)
  end

  def platforms_list
    @platforms_list ||= platforms.to_h.values.sort_by(&:title)
  end

  def select_downloads(arch: nil, os: nil, package_manager: nil, type: nil)
    downloads = []
    downloads = downloads_list.select { |d| d.arch && d.arch.downcase == arch.to_s.downcase } if arch
    downloads = downloads_list.select { |d| d.os && d.os.downcase == os.to_s.downcase } if os
    downloads = downloads_list.select { |d| d.package_manager && d.package_manager.downcase == package_manager.to_s.downcase } if package_manager
    downloads = downloads_list.select { |d| d.type && d.type.downcase == type.to_s.downcase } if type
    downloads
  end

  def to_h
    {
      downloads: downloads,
      operating_systems: operating_systems,
      package_managers: package_managers,
      platforms: platforms
    }
  end
end
