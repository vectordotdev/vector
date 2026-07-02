Treat decode failures as recoverable bad reads during disk buffer initialization seek, preventing permanent startup failure when corrupted records are encountered during `seek_to_next_record`.

authors: apurvanisal5
