class Links < OpenStruct
  def initialize(links, sources, transforms, sinks, correctness_tests, performance_tests)
    super(links)
    
    @sinks = sinks
    @sources = sources
    @transforms = transforms
    @correctness_tests = correctness_tests
    @performance_tests = performance_tests
  end

  def method_missing(method, *args, &block)
    url = super
    if url.nil?
      url = 
        case method.to_s
        when /^issue_([0-9]+)$/
          "#{REPO_ISSUES_ROOT}/#{$1}"

        when /^(.*)_sink$/
          sink = $1
          if @sinks.to_h.key?(sink.to_sym)
            "/usage/configuration/sinks/#{sink}.md"
          end

        when /^(.*)_source$/
          source = $1
          if @sources.to_h.key?(source.to_sym)
            "/usage/configuration/sources/#{source}.md"
          end

        when /^(.*_correctness)_test$/
          name = $1
          if @correctness_tests.include?(name)
            "https://github.com/timberio/vector-test-harness/tree/master/cases/#{name}"
          end

        when /^(.*_performance)_test$/
          name = $1
          if @performance_tests.include?(name)
            "https://github.com/timberio/vector-test-harness/tree/master/cases/#{name}"
          end

        when /^(.*)_transform$/
          transform = $1
          if @transforms.to_h.key?(transform.to_sym)
            "/usage/configuration/transforms/#{transform}.md"
          end

        end

      if url.nil?
        raise "#{method} link is not defined, please add it to the [links] table"
      end

      url
    else
      url
    end
  end
end