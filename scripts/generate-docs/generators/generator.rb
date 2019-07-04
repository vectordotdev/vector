#encoding: utf-8

require 'word_wrap'

class Generator
  class << self
    def say(words, color = nil)
      if color
        words = Paint[words, color]
      end

      puts "---> #{words}"
    end
  end

  def generate
    raise MethodMissingError.new
  end

  # Interpolates an existing documentation file, parsing out the section
  # directives and replacing them with updated generated content.
  #
  # This method should be used instead of #write since it allows for direct
  # markdown file editing (outside of the denoted sections).
  def interpolate(file_path, options = {})
    template = File.read(file_path)
    template = LinkGenerator.remove_link_footers(template)

    new_content = template.clone
    sections = new_content.scan(/<!-- START: (.*) -->/).flatten

    sections.each do |section|
      section_content = send(section).strip

      content =
        <<~EOF
        <!-- START: #{section} -->
        <!-- ----------------------------------------------------------------- -->
        <!-- DO NOT MODIFY! This section is generated from the /.metadata.toml -->
        <!-- via `make generate-docs`. See /DOCUMENTING.md for more info.      -->

        #{section_content}

        <!-- ----------------------------------------------------------------- -->
        <!-- END: #{section} -->
        EOF

      content.strip!

      new_content.gsub!(/<!-- START: #{section} -->(.*)<!-- END: #{section} -->/m, content)
    end

    new_content = normalize(new_content)

    if template.strip != new_content.strip
      File.write(file_path, new_content) if !options[:dry_run]
      action = options[:dry_run] ? "Will be changed" : "Changed"
      say("#{action} - #{file_path}", :green) if !options[:silent]
    else
      action = options[:dry_run] ? "Will not be changed" : "Not changed"
      say("#{action} - #{file_path}", :blue) if !options[:silent]
    end
  end

  # DEPRECATED
  #
  # Writes content for an entire documentation file. This is deprcated in
  # favor of #interpolate.
  def write(file_path, options = {})
    existing_content =
    begin
      existing_content = File.read(file_path)

      if options[:dont_clean]
        existing_content
      else
        LinkGenerator.remove_link_footers(existing_content)
      end
    rescue Errno::ENOENT
      ""
    end

    new_content = generate
    new_content = normalize(new_content)
    
    if existing_content.strip != new_content.strip
      File.write(file_path, new_content) if !options[:dry_run]
      action = options[:dry_run] ? "Will be changed" : "Changed"
      say("#{action} - #{file_path}", :green) if !options[:silent]
    else
      action = options[:dry_run] ? "Will not be changed" : "Not changed"
      say("#{action} - #{file_path}", :blue) if !options[:silent]
    end
  end

  private
    def bug_label
      "Type: Bug"
    end

    def new_feature_label
      "Type: New Feature"
    end

    def new_source_url
      new_issue_url(new_feature_label, title: "New `<name>` source")
    end

    def new_transform_url
      new_issue_url(new_feature_label, title: "New `<name>` transform")
    end

    def new_sink_url
      new_issue_url(new_feature_label, title: "New `<name>` sink")
    end

    def normalize(content)
      # Convert all ```toml definitions to ```toml since Gitbook
      # does not have a toml syntax definition and coffeescript is the closest :(
      content.gsub('```toml', '```coffeescript')
    end

    def component_label(component)
      "#{component_type(component).humanize}: #{component.name}"
    end

    def component_issues_link(component, *labels)
      label_url(component_label(component), *labels)
    end

    def component_link(component)
      "[#{component_name(component)}][#{component_short_link(component)}]"
    end

    def component_name(component)
      "`#{component.name}` #{component_type(component)}"
    end

    def component_short_link(component)
      "docs.#{component.name}_#{component_type(component)}"
    end

    def component_source_url(component)
      "#{REPO_SRC_ROOT}/#{component_type(component)}/#{component.name}.rs"
    end

    def component_type(component)
      if component.is_a?(Sink)
        "sink"
      elsif component.is_a?(Source)
        "source"
      elsif component.is_a?(Transform)
        "transform"
      else
        raise("Unknown component: #{component.inspect}")
      end
    end

    def enhancement_label
      "Type: Enhancement"
    end

    def event_type_links(types)
      types.collect do |type|
        "[`#{type}`][docs.#{type}_event]"
      end
    end

    def label_url(*labels)
      label_queries = labels.collect { |label| "label:\"#{label}\"" }
      query = "is:open is:issue #{label_queries.join(" ")}"
      REPO_ISSUES_ROOT + '?' + {"q" => query}.to_query
    end

    def new_component_issue_url(component, *labels)
      new_issue_url(component_label(component), *labels)
    end

    def new_issue_url(*args)
      params = args.last.is_a?(Hash) ? args.last.clone : {}
      labels = args
      params.merge!({"labels" => labels.join(",")})
      REPO_ISSUES_ROOT + '/new?' + params.to_query
    end

    def editorify(content)
      content = remove_markdown_links(content)
      no_wider_than(content, 78).strip.gsub("\n", "\n# ")
    end

    def no_wider_than(content, width = 80)
      WordWrap.ww(content, width)
    end

    def remove_markdown_links(content)
      content.
        gsub(/\[([^\]]+)\]\(([^)]+)\)/, '\1').
        gsub(/\[([^\]]+)\]\[([^)]+)\]/, '\1')
    end

    def say(words, color = nil)
      self.class.say(words, color)
    end

    def warning
      <<~EOF
      <!---
      !!!WARNING!!!!

      This file is autogenerated! Please do not manually edit this file.
      Instead, please modify the contents of `/.metadata.toml`.
      -->
      EOF
    end
end