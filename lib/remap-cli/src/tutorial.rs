use super::{
    repl::{resolve, Repl},
    Error,
};
use remap::{state, Runtime, Value};
use remap_functions::map;
use rustyline::{error::ReadlineError, Editor};

const HELP_TEXT: &str = r#"
Tutorial commands:
  next     Load the next tutorial
  prev     Load the previous tutorial
  exit     Exit the VRL interactive tutorial
"#;

const SYSLOG_HELP_TEXT: &str = r#"
First, parse the message to named Syslog fields using the parse_syslog function:

. = parse_syslog!(.message)

Run "." to see the new state of the event. Then you can modify the event however you like.

Some example operations:

.timestamp = to_unix_timestamp!(.timestamp)
.msgid = uuid_v4()
"#;

const JSON_HELP_TEXT: &str = r#"First, parse the message string as JSON using the parse_json function:

. = parse_json!(.message)

Run "." to see the new state of the event. Then you can modify the event however you like.

Some example operations:

del(.method); del(.host)
url = parse_url!(.referer)
del(.referer)
.referer_host = url.host
"#;

const GROK_HELP_TEXT: &str = r#"First, parse the message string using Grok with the parse_grok function:

pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
. = parse_grok!(.message, pattern)

Run "." to see the new state of the event. Then you can modify the event however you like.

Some example operations:

.timestamp = to_unix_timestamp(to_timestamp!(.timestamp))
.message = downcase(.message)
"#;

struct Tutorial {
    title: &'static str,
    help_text: &'static str,
    object: Value,
}

pub fn tutorial() -> Result<(), Error> {
    let mut index = 0;
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    let syslog_tut = Tutorial {
        title: "Syslog messages",
        help_text: SYSLOG_HELP_TEXT,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "<31>2 2021-01-21T18:46:59.991Z acmecorp.org auth 7726 ID312 - Uh oh, Spaghetti-o's"
        ]),
    };

    let json_tut = Tutorial {
        title: "JSON logs",
        help_text: JSON_HELP_TEXT,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "{\"host\":\"75.58.250.157\",\"user-identifier\":\"adalovelace1337\",\"datetime\":\"21/Jan/2021:18:46:59 -0700\",\"method\":\"PATCH\",\"request\":\"/wp-admin\",\"protocol\":\"HTTP/2.0\",\"status\":401,\"bytes\":20320,\"referer\":\"http://www.evilcorp.org/sql-injection\"}",
        ]),
    };

    let grok_tut = Tutorial {
        title: "Grok patterns",
        help_text: GROK_HELP_TEXT,
        object: Value::from(map![
            "message": "2021-01-21T18:46:59.991Z error Too many cooks in the kitchen"
        ]),
    };

    let mut tutorials = vec![syslog_tut, json_tut, grok_tut];

    println!("Welcome to the Vector Remap Language interactive tutorial!\n");

    print_tutorial_help_text(index, &tutorials, false);

    loop {
        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "exit" || line == "quit" => break,
            Ok(line) => {
                rl.add_history_entry(line);

                match line {
                    "help" => help(),
                    "next" => {
                        if (index + 1) == tutorials.len() {
                            println!(
                                "You've finished all the steps! Taking you back to the beginning\n"
                            );
                            index = 0;
                            print_tutorial_help_text(index, &tutorials, true);
                        } else {
                            index = index.saturating_add(1);
                            print_tutorial_help_text(index, &tutorials, true,);
                        }
                    }
                    "prev" => {
                        if index == 0 {
                            println!("You're back at the beginning!\n");
                        }

                        index = index.saturating_sub(1);
                        print_tutorial_help_text(index, &tutorials, true);
                    }
                    "" => continue,
                    command => {
                        let object = &mut tutorials[index].object;
                        let value = resolve(Some(object), &mut rt, command, &mut compiler_state);
                        println!("{}\n", value);
                    }
                };
            }
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                println!("unable to read line: {}", err);
                break;
            }
        }
    }

    Ok(())
}

fn help() {
    println!("{}", HELP_TEXT);
}

fn print_tutorial_help_text(index: usize, tutorials: &Vec<Tutorial>, include_topline: bool) {
    let tut = &tutorials[index];

    if include_topline {
        println!("------------");
    }

    println!(
        "\nTutorial {}: {}\n\n{}\nEvent:\n{}\n",
        index + 1,
        tut.title,
        tut.help_text,
        tut.object
    );
}
