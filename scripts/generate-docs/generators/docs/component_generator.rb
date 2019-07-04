#encoding: utf-8

require 'word_wrap'
require_relative "../generator"

module Docs
  class ComponentGenerator < Generator
    attr_reader :guides

    def initialize(guides)
      @guides = guides
    end

    private
      def alternatives_section(alternatives)
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

      def resources_section(component)
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

      def troubleshooting_section(component)
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
        
        #{alternatives_section(component.alternatives)}
        EOF
        content.strip
      end
  end
end