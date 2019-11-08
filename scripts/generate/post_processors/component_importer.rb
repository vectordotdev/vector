#encoding: utf-8

module PostProcessors
  # Automatically imports components.
  #
  # Imports can only happen once, which is not easy when we use templates.
  # It's also easy to forget an import, which does not throw an error. This
  # post processor ensures imports are always present.
  class ComponentImporter
    IMPORTS = {
      'CodeHeader' => "import CodeHeader from '@site/src/components/CodeHeader';",
      'Components' => "import Components from '@site/src/components/Components';",
      'Component' => "import Component from '@site/src/components/Component';",
      'Diagram' => "import Diagram from '@site/src/components/Diagram';",
      'Fields' => "import Fields from '@site/src/components/Fields';",
      'Field' => "import Field from '@site/src/components/Field';",
      'Jump' => "import Jump from '@site/src/components/Jump';",
      'Tabs' => "import Tabs from '@theme/Tabs';",
      'TabItem' => "import TabItem from '@theme/TabItem';"
    }

    class << self
      def import!(content)
        IMPORTS.each do |tag, import|
          if content.include?("<#{tag}") && !content.include?(import)
            content.sub!("<#{tag}", "#{import}\n\n<#{tag}")
          end
        end

        content
      end
    end
  end
end