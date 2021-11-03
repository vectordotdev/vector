---
date: "2020-07-13"
title: "Vector gracefully exits when specific sources finish"
description: "One step closer to Vector replacing awk and sed!"
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2533]
release: "0.10.0"
badges:
  type: "enhancement"
  sources: ["stdin"]
---

We heard from some folks they were using Vector as a data processor in command line scripts!

**Good for you, UNIX hackers 👩‍💻!**

Now, in 0.10.0, you can use Vector in standard UNIX pipelines much easier!

```bash
banana@tree:/$ echo "awk, sed the Vic" | vector --config test.toml --quiet
{"host":"tree","message":"awk, sed the Vic","source_type":"stdin","timestamp":"2020-05-04T20:43:59.522211979Z"}
```

## Future Outlook

We've been exploring options for expanding `vector generate` to allow users to specify options from the command line. For example:

```bash
banana@tree:/$ vector generate stdin//console(encoding=json)
```

Once this happens, it seems inevitable we'll add something like this eventually:

```bash
banana@tree:/$ vector eval stdin//console(encoding=json)
```

Want to contribute? [Discussion here!][urls.vector_generate_arguments_issue]

[urls.vector_generate_arguments_issue]: https://github.com/vectordotdev/vector/issues/1966
