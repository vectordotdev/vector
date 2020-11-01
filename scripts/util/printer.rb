require "paint"

module Util
  module Printer
    PROMPT = "---> "
    INDENT = "     "
    SEPARATOR = "-" * 80
    TITLE_PROMPT = "#### "

    extend self

    def error!(message)
      Printer.say(message, color: :red)
      exit(1)
    end

    def Printer.get(words, choices = nil)
      question = "#{words.strip}"

      if !choices.nil?
        question += " (" + choices.join("/") + ")"
      end

      Printer.say(question)

      print INDENT

      input = gets().chomp

      if choices && !choices.include?(input)
        Printer.say("You must enter one of #{choices.join(", ")}", color: :red)
        Printer.get(words, choices)
      else
        input
      end
    end

    def invalid(words)
      Printer.say(words, color: :yellow)
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
      Printer.say(words, color: :green)
    end

    def title(words)
      separate(color: :cyan)
      Printer.say(words, color: :cyan, prompt: TITLE_PROMPT)
    end
  end
end
