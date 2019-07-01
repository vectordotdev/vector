require 'word_wrap'

class Generator
  attr_reader :guides

  def initialize(guides)
    @guides = guides
  end

  def generate
    raise MethodMissingError.new
  end

  def _generate_new(file_path)
    template = File.read(file_path)
    new_content = template.clone
    sections = new_content.scan(/<!-- START: (.*) -->/).flatten

    sections.each do |section|
      section_content = send(section).strip

      content =
        <<~EOF
        <!-- START: #{section} -->
        <!-- ----------------------------------------------------------------- -->
        <!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

        #{section_content}

        <!-- ----------------------------------------------------------------- -->
        <!-- END: #{section} -->
        EOF

      content.strip!

      new_content.gsub!(/<!-- START: #{section} -->(.*)<!-- END: #{section} -->/m, content)
    end

    if template != new_content
      File.write(file_path, new_content)
      puts Paint["-> ✔ Updated: #{file_path}", :green]
    else
      puts Paint["-> - Not changed: #{file_path}", :yellow]
    end
  end

  private

    def alternatives(alternatives)
      if alternatives.any?
        alternative_links = alternatives.collect do |alternative|
          "* [#{component_name(alternative)}][#{component_short_link(alternative)}]"
        end

        content = <<~EOF

        ### Alternatives

        Finally, consider the following alternatives:

        #{alternative_links.join("\n")}
        EOF

        content.strip
      else
        ""
      end
    end

    def beta(component)
      if component.beta?
        content = <<~EOF
        {% hint style="warning" %}
        The #{component_name(component)} is in beta. Please see the current [enhancements](#{component_issues_link(component, enhancement_label)}) and [bugs](#{component_issues_link(component, bug_label)}) for known issues. We kindly ask that you [add any missing issues](#{new_component_issue_url(component)}) as it will help shape the roadmap of this component.
        {% endhint %}
        EOF
        content.strip
      else
        ""
      end
    end

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

    def guides_section(component)
      guide_links =
        guides.
          select { |guide| guide.send(component.type.pluralize).include?(component.name) }.
          collect do |guide|
            "* [**#{guide.title} Guide**](#{guide.file_path})"
          end

      content =
        if guide_links.any?
          <<~EOF
          ## Guides

          #{guide_links.join("\n")}
          EOF
        else
          ""
        end

      content.lstrip.strip
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

    def example_section(component, prefix = nil)
      return "" if component.examples.empty?

      content =
        if component.examples.length > 1
          tabs =
            component.examples.collect do |example|
              content =
                <<~EOF
                {% tab title="#{example.name}" %}
                #{example.body}
                {% endtab %}
                EOF

              content.strip
            end

          <<~EOF
          {% tabs %}
          #{tabs.join("\n")}
          {% endtabs %}
          EOF
        else
          component.examples.first.body
        end

      <<~EOF
      ## Examples

      #{prefix}

      #{content.strip}
      EOF
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

    def resource_links(resources)
      links = resources.collect do |resource|
        if resource.short_link
          "* [**#{resource.name}**][url.#{resource.short_link}]"
        elsif resource.url
          "* [**#{resource.name}**](#{resource.url})"
        else
          raise "Resource #{resource.name} does not have a URL!"
        end
      end
      links.join("\n")
    end

    def resources(component)
      content = <<~EOF
      ## Resources

      * [**Issues**](#{component_issues_link(component)}) - [enhancements](#{component_issues_link(component, enhancement_label)}) - [bugs](#{component_issues_link(component, bug_label)})
      * [**Source code**](#{component_source_url(component)})
      #{resource_links(component.resources)}
      EOF
      content.strip
    end

    def tags(component)
      tags = []

      tags << "`status: #{component.beta? ? "beta" : "stable"}`"

      if component.respond_to?("input_types") && component.input_types.any?
        tags << "`input: #{component.input_types.to_sentence}`"
      end

      if component.respond_to?("output_types") && component.output_types.any?
        tags << "`output: #{component.output_types.to_sentence}`"
      end

      if component.respond_to?("delivery_guarantee")
        tags << "`guarantee: [#{component.delivery_guarantee}][#{component.delivery_guarantee}_delivery]`"
      end

      tags.join(" ")
    end

    def troubleshooting(component)
      content = <<~EOF
      ## Troubleshooting

      The best place to start with troubleshooting is to check the
      [Vector logs][docs.monitoring_logs]. This is typically located at
      `/var/log/vector.log`, then proceed to follow the
      [Troubleshooting Guide][docs.troubleshooting].

      If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
      issue, please:

      1. Check for any [open #{component_type(component)} issues](#{component_issues_link(component)}).
      2. [Search the forum][url.search_forum] for any similar issues.
      2. Reach out to the [community][url.community] for help.
      
      #{alternatives(component.alternatives)}
      EOF
      content.strip
    end

    def warning
      <<~EOF
      <!---
      !!!WARNING!!!!

      This file is autogenerated! Please do not manually edit this file.
      Instead, please modify the contents of `scripts/metadata.toml`.
      -->
      EOF
    end
end