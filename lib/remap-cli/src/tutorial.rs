use super::{repl::Repl, Error};
use remap::{state, Formatter, Program, Runtime, Value};
use remap_functions::{all as funcs, map};
use rustyline::{error::ReadlineError, Editor};

const HELP_TEXT: &str = r#"
Tutorial commands:
  next     Load the next tutorial
  prev     Load the previous tutorial
  exit     Exit the VRL interactive tutorial
"#;

struct Tutorial {
    title: &'static str,
    help_text: &'static str,
    object: Value,
}

pub fn tutorial() -> Result<(), Error> {
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    let syslog_tut = Tutorial {
        title: "Syslog messages",
        help_text: r#"First, parse the message to named Syslog fields using the parse_syslog function:

. = parse_syslog!(.message)

Then, you can modify the event however you like. For example, you can convert the timestamp to a Unix timestamp:

.timestamp = to_unix_timestamp!(.timestamp)

You can overwrite the msgid field with a unique ID:

.msgid = uuid_v4()
"#,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "<31>2 2021-01-21T18:46:59.991Z acmecorp.org auth 7726 ID312 - Uh oh, Spaghetti-o's"
        ]),
    };

    let json_tut = Tutorial {
        title: "JSON logs",
        help_text: r#"First, parse the message string as JSON using the parse_json function:

. = parse_json!(.message)

Then, you can modify the event however you like. For example, you can delete some fields:

del(.method); del(.host)

You can parse the referer URL:

url = parse_url!(.referer)
.host = url.host
"#,
        object: Value::from(map![
            "timestamp": "2021-01-21T18:46:59.991Z",
            "message": "{\"host\":\"75.58.250.157\",\"user-identifier\":\"adalovelace1337\",\"datetime\":\"21/Jan/2021:18:46:59 -0700\",\"method\":\"PATCH\",\"request\":\"/wp-admin\",\"protocol\":\"HTTP/2.0\",\"status\":401,\"bytes\":20320,\"referer\":\"http://www.evilcorp.org/sql-injection\"}",
        ]),
    };

    let mut tutorials = vec![syslog_tut, json_tut];

    println!("Welcome to the Vector Remap Language interactive tutorial!\n");

    let mut index: usize = 0;

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
                            print_tutorial_help_text(index, &tutorials);
                        } else {
                            index = index.saturating_add(1);
                            print_tutorial_help_text(index, &tutorials);
                        }
                    }
                    "prev" => {
                        index = index.saturating_sub(1);
                        print_tutorial_help_text(index, &tutorials);
                    }
                    "" => continue,
                    command => {
                        let object = &mut tutorials[index].object;
                        let value = resolve(object, &mut rt, command, &mut compiler_state);
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

fn print_tutorial_help_text(index: usize, tutorials: &Vec<Tutorial>) {
    let tut = &tutorials[index];
    println!(
        "Tutorial {}: {}\n\n{}\nEvent:\n{}\n",
        index + 1,
        tut.title,
        tut.help_text,
        tut.object
    );
}

fn resolve(
    object: &mut Value,
    runtime: &mut Runtime,
    program: &str,
    state: &mut state::Compiler,
) -> String {
    let program = match Program::new_with_state(program.to_owned(), &funcs(), None, true, state) {
        Ok((program, _)) => program,
        Err(diagnostics) => return Formatter::new(program, diagnostics).colored().to_string(),
    };

    match runtime.run(object, &program) {
        Ok(value) => value.to_string(),
        Err(err) => err.to_string(),
    }
}
