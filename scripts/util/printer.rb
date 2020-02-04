module Printer
  PROMPT = "---> "
  INDENT = "     "
  SEPARATOR = "-" * 80
  TITLE_PROMPT = "#### "

  extend self

  def error!(message)
    say(message, color: :red)
    exit(1)
  end

  def get(words, choices = nil)
    question = "#{words.strip}"

    if !choices.nil?
      question += " (" + choices.join("/") + ")"
    end

    say(question)

    print INDENT

    input = gets().chomp

    if choices && !choices.include?(input)
      say("You must enter one of #{choices.join(", ")}", color: :red)
      get(words, choices)
    else
      input
    end
  end

  def invalid(words)
    say(words, color: :yellow)
  end

  def say(words, color: nil, new: true, prompt: PROMPT)
    prefix = new ? prompt : INDENT

    if color
      words = Paint[prefix + words, color]
    else
      words = prefix + words
    end

    puts words.gsub("\n", "\n#{INDENT}")
  end

  def separate(color: nil)
    string = SEPARATOR

    if color
      string = Paint[string, color]
    end

    puts ""
    puts string
  end

  def success(words)
    say(words, color: :green)
  end

  def title(words)
    separate(color: :cyan)
    say(words, color: :cyan, prompt: TITLE_PROMPT)
  end
end
