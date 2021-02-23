use super::{
    common::{open_url, print_function_list, Repl},
    Error,
};
use crate::timestamp;
use indoc::indoc;
use rustyline::{error::ReadlineError, Editor};
use vrl::{diagnostic::Formatter, state, Runtime, Target, Value};
use vrl_compiler::value;

struct Tutorial {
    section: usize,
    id: usize,
    title: &'static str,
    help_text: &'static str,
    // The URL endpoint (https://vrl.dev/:endpoint) for finding out more
    docs: &'static str,
    initial_event: Value,
    correct_answer: Value,
    cheat: &'static str,
}

impl Tutorial {
    fn number(&self) -> String {
        format!("{}.{}", self.section, self.id)
    }
}

pub fn tutorial() -> Result<(), Error> {
    let mut index = 0;
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Runtime::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new("> ")));

    let mut tutorials = tutorials();

    // Tutorial intro
    clear_screen();
    println!("{}", INTRO_TEXT);

    // Wait for "next" to continue
    {
        let mut rl = Editor::<Repl>::new();

        'intro: loop {
            match rl.readline("🚀 ").as_deref() {
                Ok(line) if line == "exit" || line == "quit" => {
                    println!("\nSee you next time! And don't forget to check out https://vrl.dev for more info!\n");
                    return Ok(());
                }
                Ok(line) if line == "start" => {
                    clear_screen();
                    break 'intro;
                }
                _ => {
                    println!("\nDidn't recognize that input. Type `next` and hit Enter to move on or `exit` to leave the VRL tutorial.\n");
                    continue;
                }
            }
        }
    }

    print_tutorial_help_text(0, &tutorials);

    'outer: loop {
        let readline = rl.readline("> ");
        match readline.as_deref() {
            Ok(line) if line == "exit" || line == "quit" => break 'outer,
            Ok(line) => {
                rl.add_history_entry(line);

                match line {
                    "" => continue,
                    "help" => help(),
                    "next" => {
                        clear_screen();

                        // End if no more tutorials are left, or else increment the index
                        if (index + 1) == tutorials.len() {
                            println!("\n\nCongratulations! You've successfully completed the VRL tutorial.\n");
                            break;
                        } else {
                            index = index.saturating_add(1);
                        }

                        print_tutorial_help_text(index, &tutorials);
                    }
                    "prev" => {
                        clear_screen();

                        if index == 0 {
                            println!("\n\nYou're back at the beginning!\n\n");
                        }

                        index = index.saturating_sub(1);
                        print_tutorial_help_text(index, &tutorials);
                    }
                    "docs" => {
                        let tut = &tutorials[index];
                        let endpoint = &tut.docs;
                        let docs_url = format!("https://vrl.dev/{}", endpoint);

                        open_url(&docs_url);
                    }
                    "remind" => {
                        let tut = &tutorials[index];
                        let help_text = tut.help_text;

                        println!("\n{}", help_text);
                    }
                    "cheat" => {
                        let tut = &tutorials[index];
                        let cheat_text = tut.cheat;
                        println!(
                            "\nSince you insist on taking the easy way out...\n\n{}\n",
                            cheat_text
                        );
                    }
                    "funcs" => {
                        print_function_list();
                    }
                    command => {
                        let tut = &mut tutorials[index];
                        let event = &mut tut.initial_event;
                        let correct_answer = &tut.correct_answer;

                        // Purely for debugging
                        if command == "cheat" {
                            clear_screen();
                            println!("{}", correct_answer);
                        }

                        match resolve_to_value(event, &mut rt, command, &mut compiler_state) {
                            Ok(result) => {
                                if event == correct_answer {
                                    clear_screen();

                                    println!(
                                        "CORRECT! You've wisely ended up with this event:\n\n{}\n",
                                        event
                                    );

                                    // Exit if no more tutorials are left, otherwise move on to the next one
                                    if (index + 1) == tutorials.len() {
                                        println!("Congratulations! You've successfully completed the VRL tutorial.\n");
                                        break 'outer;
                                    } else {
                                        println!(
                                            "You've now completed tutorial {} out of {}.\nType `next` and hit Enter to move on to tutorial number {} or `exit` to leave the VRL tutorial.\n",
                                            index + 1,
                                            tutorials.len(),
                                            index + 2,
                                        );

                                        // Wait for "next" to continue
                                        {
                                            let mut rl = Editor::<Repl>::new();

                                            'next: loop {
                                                match rl.readline("🚀 ").as_deref() {
                                                    Ok(line)
                                                        if line == "exit" || line == "quit" =>
                                                    {
                                                        break 'outer
                                                    }
                                                    Ok(line) if line == "next" => {
                                                        clear_screen();
                                                        break 'next;
                                                    }
                                                    _ => {
                                                        println!("\nDidn't recognize that input. Type `next` and hit Enter to move on or `exit` to leave the VRL tutorial.\n");
                                                        continue;
                                                    }
                                                }
                                            }
                                        }

                                        index = index.saturating_add(1);
                                        print_tutorial_help_text(index, &tutorials);
                                    }
                                } else {
                                    println!("{}", result);
                                }
                            }
                            Err(err) => {
                                println!("{}", err);
                            }
                        }
                    }
                };
            }
            Err(ReadlineError::Interrupted) => break 'outer,
            Err(ReadlineError::Eof) => break 'outer,
            Err(err) => {
                println!("unable to read line: {}", err);
                break 'outer;
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

    println!(
        "Tutorial {}: {}\n\n{}\nInitial event object:\n{}\n",
        tut.number(),
        tut.title,
        tut.help_text,
        tut.initial_event
    );
}

#[cfg(unix)]
fn clear_screen() {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
}

#[cfg(windows)]
fn clear_screen() {
    print!("\x1b[2J");
}

// This function reworks the resolve function in repl.rs to return a Result rather than a String. If the Result is
// Ok, the value is used to check whether the current event is equal to the "correct" answer.
pub fn resolve_to_value(
    object: &mut dyn Target,
    runtime: &mut Runtime,
    program: &str,
    state: &mut state::Compiler,
) -> Result<Value, String> {
    let program = match vrl::compile_with_state(program, &stdlib::all(), state) {
        Ok(program) => program,
        Err(diagnostics) => {
            let msg = Formatter::new(program, diagnostics).colored().to_string();
            return Err(msg);
        }
    };

    match runtime.resolve(object, &program) {
        Ok(v) => Ok(v),
        Err(err) => Err(err.to_string()),
    }
}

// Help text
const HELP_TEXT: &str = r#"
Tutorial commands:

  .        Show the current value of the event
  docs     Open documentation for the current tutorial in your browser
  funcs    Display a list of all VRL functions
  remind   Display the tutorial's help text again
  cheat    Take the easy way out and see how to solve the exercise
  next     Load the next tutorial
  prev     Load the previous tutorial
  exit     Exit the VRL interactive tutorial shell
"#;

const INTRO_TEXT: &str = r#"Welcome to the Vector Remap Language (VRL)
interactive tutorial!

VRL is a language for working with observability data (logs and metrics) in
Vector. Here, you'll be guided through a series of exercises that teach you how
to use VRL by solving problems. At any time, you can type `help` to get a list
of available commands.

Type `start` and hit Enter to begin.
"#;

fn tutorials() -> Vec<Tutorial> {
    let assignment_tut = Tutorial {
        section: 1,
        id: 1,
        title: "Assigning values to fields",
        docs: "expressions/#assignment",
        help_text: indoc! {r#"
            In VRL, you can assign values to fields like this:

            > .field = "value"

            TASK:
            - Assign the string "hello" to the field `message`
        "#},
        initial_event: value![{}],
        correct_answer: value![{"message": "hello"}],
        cheat: r#".message = "hello""#,
    };

    let deleting_fields_tut = Tutorial {
        section: 1,
        id: 2,
        title: "Deleting fields",
        docs: "functions/#del",
        help_text: indoc! {r#"
            You can delete fields from events using the `del` function:

            > del(.field)

            TASK:
            - Delete fields `one` and `two` from the event
        "#},
        initial_event: value![{"one": 1, "two": 2, "three": 3}],
        correct_answer: value![{"three": 3}],
        cheat: "del(.one); del(.two)",
    };

    let exists_tut = Tutorial {
        section: 1,
        id: 3,
        title: "Existence checking",
        docs: "functions/#exists",
        help_text: indoc! {r#"
            You can check whether a field has a value using the `exists`
            function:

            > exists(.field)

            TASK:
            - Make the event consist of just one `exists` field that indicates
              whether the `not_empty` field exists

            HINT:
            - You may need to use the `del` function too!
        "#},
        initial_event: value![{"not_empty": "This value does exist!"}],
        correct_answer: value![{"exists": true}],
        cheat: indoc! {r#"
            .exists = exists(.not_empty)
            del(.not_empty)
        "#},
    };

    let type_coercion_tut = Tutorial {
        section: 1,
        id: 4,
        title: "Type coercion",
        docs: "functions/#coerce-functions",
        help_text: indoc! {r#"
            You can coerce VRL values into other types using the `to_*` coercion
            functions (`to_bool`, `to_string`, etc.).

            TASK:
            - Coerce all of the fields in this event into the type suggested by
              the key (convert the `boolean` field into a Boolean and so on)

            HINT:
            - The coercion functions are fallible, so be sure to handle errors!
            - Use the "funcs" command to see a list of all VRL functions
        "#},
        initial_event: value![{"boolean": "yes", "integer": "1337", "float": "42.5", "string": true}],
        correct_answer: value![{"boolean": true, "integer": 1337, "float": 42.5, "string": "true"}],
        cheat: indoc! {r#"
            .boolean = to_bool!(.boolean)
            .integer = to_int!(.integer)
            .float = to_float!(.float)
            .string = to_string!(.string)
        "#},
    };

    let parse_json_tut = Tutorial {
        section: 2,
        id: 1,
        title: "Parsing JSON",
        docs: "functions/#parse_json",
        help_text: indoc! {r#"
            You can parse inputs to JSON in VRL using the `parse_json` function:

            > parse_json(.field)

            `parse_json` is fallible, so make sure to handle potential errors!

            TASK:
            - Set the value of the event to the `message` field parsed as JSON
        "#},
        initial_event: value![{"message": r#"{"severity":"info","message":"Coast is clear"}"#, "timestamp": "2021-02-16T00:25:12.728003Z"}],
        correct_answer: value![{"severity": "info", "message": "Coast is clear"}],
        cheat: ". = parse_json!(string!(.message))",
    };

    let t1 = "2020-12-19T21:48:09.004Z";
    let ts1 = timestamp!(t1);
    let msg1 = format!(
        "<12>3 {} initech.io su 4015 ID81 - TPS report missing cover sheet",
        t1
    );

    let parse_syslog_tut = Tutorial {
        section: 2,
        id: 2,
        title: "Parsing Syslog",
        docs: "functions/#parse_syslog",
        help_text: indoc! {r#"
            You can parse Syslog messages into named fields using the `parse_syslog`
            function:

            > parse_syslog(.field)

            TASK:
            - Set the value of the event to the `message` field parsed from Syslog

            HINTS:
            - `parse_syslog` is fallible, so make sure to handle potential errors!
            - `parse_syslog` can only take a string
        "#},
        initial_event: value![{"message": msg1, "timestamp": t1}],
        correct_answer: value![{"appname": "su", "facility": "user", "hostname": "initech.io", "message": "TPS report missing cover sheet", "msgid": "ID81", "procid": 4015, "severity": "warning", "timestamp": ts1, "version": 3}],
        cheat: ". = parse_syslog!(string!(.message))",
    };

    let parse_key_value_tut = Tutorial {
        section: 2,
        id: 3,
        title: "Parsing key-value logs",
        docs: "functions/#parse_key_value",
        help_text: indoc! {r#"
            You can parse key-value strings of the form "foo=bar bar=baz" into an
            object using the `parse_key_value` function:

            > parse_key_value(.field)

            TASK:
            - Set the value of the event to the `message` field parsed from key-value
              format

            HINTS:
            - `parse_key_value` is fallible, so make sure to handle potential errors!
            - `parse_key_value` can only take a string
        "#},
        initial_event: value![{"message": r#"@timestamp="2020-12-19T21:48:09.004Z" severity=info msg="Smooth sailing over here""#}],
        correct_answer: value![{"@timestamp": "2020-12-19T21:48:09.004Z", "msg": "Smooth sailing over here", "severity": "info"}],
        cheat: ". = parse_key_value!(string!(.message))",
    };

    let t2 = "2021-01-03T08:01:47.004Z";
    let ts2 = timestamp!(t2);
    let msg2 = format!(
        "<12>3 {} initech.io su 4015 ID81 - TPS report missing cover sheet",
        t2
    );

    let transform_syslog_tut = Tutorial {
        section: 3,
        id: 1,
        title: "Transforming Syslog logs",
        docs: "functions/#parse_syslog",
        help_text: indoc! {r#"
            Thus far, we've mostly *parsed* events from one format into another. Now
            we're going to start *transforming* events.

            TASK:
            - Parse the `message` field (Syslog format) into a VRL object and set the event to that value
            - Set the `severity` field to "info"
            - Delete the `version`, `msgid`, and `procid` fields
            - Convert the `message` field to all lowercase using `downcase`
        "#},
        initial_event: value![{"message": msg2, "timestamp": t2}],
        correct_answer: value![{"appname": "su", "facility": "user", "hostname": "initech.io", "message": "tps report missing cover sheet", "severity": "info", "timestamp": ts2}],
        cheat: indoc! {r#"
            . = parse_syslog!(string!(.message))
            .severity = "info"
            del(.version); del(.msgid); del(.procid)
            .message = downcase(string!(.message))
        "#},
    };

    let t3 = "2021-03-04T21:13:42.001Z";
    let ts3 = timestamp!(t3);
    let msg3 = "{\"status\":\"200\",\"method\":\"POST\",\"endpoint\":\"/purchases\",\"username\":\"tonydanza\",\"bytes\":\"1337\"}";

    let transform_json_tut = Tutorial {
        section: 3,
        id: 2,
        title: "Transforming JSON logs",
        docs: "functions/#parse_json",
        help_text: indoc! {r#"
            TASKS:
            - Parse the `message` field (JSON string) into a VRL object and set the event to that value
            - Delete the `username` field
            - Convert the `status` and `bytes` fields to integers

            HINT:
            - Use the "funcs" command to see a list of all VRL functions
        "#},
        initial_event: value![{"message": msg3, "timestamp": ts3}],
        correct_answer: value![{"bytes": 1337, "endpoint": "/purchases", "method": "POST", "status": 200}],
        cheat: indoc! {r#"
            . = parse_json!(string!(.message))
            del(.username)
            .status = to_int!(.status)
            .bytes = to_int!(.bytes)
        "#},
    };

    vec![
        assignment_tut,
        deleting_fields_tut,
        exists_tut,
        type_coercion_tut,
        parse_json_tut,
        parse_syslog_tut,
        parse_key_value_tut,
        transform_syslog_tut,
        transform_json_tut,
    ]
}

#[macro_export]
macro_rules! timestamp {
    ($ts:tt) => {
        vrl::Value::Timestamp(chrono::DateTime::parse_from_rfc3339($ts).unwrap().into())
    };
}
