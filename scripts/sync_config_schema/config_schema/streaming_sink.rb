require_relative "section"
require_relative "sink"

class StreamingSink < Sink
  def initialize(hash)
    super(hash)

    @sections << Section.new({
      "title" => "Streaming",
      "body" =>
        <<~EOF
        Events will be streamed in a real-time, one-by-one fashiong, making
        events immediately available. They will not be batched.
        EOF
    })
  end

  def type
    "sink"
  end
end