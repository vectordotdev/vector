use super::{
    repl::{resolve, Repl},
    Error,
};
use remap::{state, Runtime, Value};
use remap_functions::map;
use rustyline::{error::ReadlineError, Editor};

struct Tutorial {
    number: &'static str, // Making this a string allows for 1.1, 2.5, etc.
    title: &'static str,
    help_text: &'static str,
    correct_answer: Option<&'static str>,
    object: Value,
}

pub fn tutorial() -> Result<(), Error> {
    let mut index = 0;
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    let assignment_tut = Tutorial {
        number: "1.1",
        title: "Assigning values",
        help_text: ASSIGNMENT_TEXT,
        correct_answer: Some(r#".foo = "bar""#),
        object: Value::from(map![]),
    };

    let deletion_tut = Tutorial {
        number: "1.2",
        title: "Deleting values",
        help_text: DELETION_TEXT,
        correct_answer: Some(r#"del(.password)"#),
        object: Value::from(
            map!["timestamp": "2021-01-21T18:46:59.991Z", "method": "POST", "endpoint": "/inventions", "user": "adalovelace", "password": "opensesame"],
        ),
    };

    let syslog_tut = Tutorial {
        number: "2.1",
        title: "Syslog messages",
        help_text: SYSLOG_HELP_TEXT,
        correct_answer: None,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "<31>2 2021-01-21T18:46:59.991Z acmecorp.org auth 7726 ID312 - Uh oh, Spaghetti-o's"
        ]),
    };

    let json_tut = Tutorial {
        number: "2.2",
        title: "JSON logs",
        help_text: JSON_HELP_TEXT,
        correct_answer: None,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "{\"host\":\"75.58.250.157\",\"user-identifier\":\"adalovelace1337\",\"datetime\":\"21/Jan/2021:18:46:59 -0700\",\"method\":\"PATCH\",\"request\":\"/wp-admin\",\"protocol\":\"HTTP/2.0\",\"status\":401,\"bytes\":20320,\"referer\":\"http://www.evilcorp.org/sql-injection\"}",
        ]),
    };

    let grok_tut = Tutorial {
        number: "2.3",
        title: "Grok patterns",
        help_text: GROK_HELP_TEXT,
        correct_answer: None,
        object: Value::from(map![
            "message": "2021-01-21T18:46:59.991Z error Too many cooks in the kitchen"
        ]),
    };

    let mut tutorials = vec![assignment_tut, deletion_tut, syslog_tut, json_tut, grok_tut];

    println!("\nWelcome to the Vector Remap Language interactive tutorial!\n~~~~~~~~~~~~~~~~~~~\n");

    print_tutorial_help_text(index, &tutorials);

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
                        } else {
                            index = index.saturating_add(1);
                        }

                        print_tutorial_help_text(index, &tutorials);
                    }
                    "prev" => {
                        if index == 0 {
                            println!("You're back at the beginning!\n");
                        }

                        index = index.saturating_sub(1);
                        print_tutorial_help_text(index, &tutorials);
                    }
                    "" => continue,
                    command => {
                        let tut = &mut tutorials[index];
                        let object = &mut tut.object;
                        let value = resolve(Some(object), &mut rt, command, &mut compiler_state);
                        println!("{}\n", value);

                        if let Some(correct_answer) = tut.correct_answer {
                            if command == correct_answer {
                                println!("That is correct!");
                                index = index.saturating_add(1);

                                println!("Current event state:\n{}", object);

                                print_tutorial_help_text(index, &tutorials);
                            }
                        }
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

fn print_tutorial_help_text(index: usize, tutorials: &[Tutorial]) {
    let tut = &tutorials[index];

    if index != 0 {
        println!("------------");
    }

    println!(
        "\nTutorial {}: {}\n{}\nEvent:\n{}\n",
        tut.number, tut.title, tut.help_text, tut.object
    );
}

// Help text
const HELP_TEXT: &str = r#"
Tutorial commands:
  next     Load the next tutorial
  prev     Load the previous tutorial
  exit     Exit the VRL interactive tutorial
"#;

const ASSIGNMENT_TEXT: &str = r#"In VRL, you can assign values to fields like this:

.field = "value"

Assign the string "bar" to the field "foo."
"#;

const DELETION_TEXT: &str = r#"You can delete fields using the `del` function and specifying the field, like this:

del(.foo)

Delete the "password" field from this HTTP server log line.
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
