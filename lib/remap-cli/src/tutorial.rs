use super::{repl::Repl, Error};
use remap::{state, Formatter, Object, Program, Runtime, Value};
use remap_functions::{all as funcs, map};
use rustyline::{error::ReadlineError, Editor};

struct Tutorial {
    number: &'static str, // Making this a string allows for 1.1, 2.5, etc.
    title: &'static str,
    help_text: &'static str,
    correct_answer: Value,
    initial_event: Value,
}

pub fn tutorial() -> Result<(), Error> {
    let mut index = 0;
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    let mut tutorials = tutorial_list();

    println!("\nWelcome to the Vector Remap Language interactive tutorial!\n");

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
                            println!("\n\nYou're back at the beginning!\n\n");
                        }

                        index = index.saturating_sub(1);
                        print_tutorial_help_text(index, &tutorials);
                    }
                    "" => continue,
                    command => {
                        let tut = &mut tutorials[index];
                        let event = &mut tut.initial_event;
                        match resolve(event, &mut rt, command, &mut compiler_state) {
                            Ok(result) => {
                                if event == &tut.correct_answer {
                                    println!("\n\nCORRECT! You have wisely ended up with this event:\n{}\n", event);

                                    if (index + 1) == tutorials.len() {
                                        println!("\n\nCongratulations! You've successfully completed the VRL tutorial.\n");
                                        break;
                                    } else {
                                        println!("Moving on to the next exercise...\n\n");
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

    println!(
        "Tutorial {}: {}\n\n{}\nInitial event object:\n{}\n",
        tut.number, tut.title, tut.help_text, tut.initial_event
    );
}

// This function reworks the resolve function in repl.rs to return a Result rather than a String. If the Result is
// Ok, the value is used to check whether the current event is equal to the "correct" answer.
pub fn resolve(
    object: &mut impl Object,
    runtime: &mut Runtime,
    program: &str,
    state: &mut state::Compiler,
) -> Result<Value, String> {
    let program = match Program::new_with_state(program.to_owned(), &funcs(), None, true, state) {
        Ok((program, _)) => program,
        Err(diagnostics) => {
            let msg = Formatter::new(program, diagnostics).colored().to_string();
            return Err(msg);
        }
    };

    match runtime.run(object, &program) {
        Ok(v) => Ok(v),
        Err(err) => Err(err.to_string()),
    }
}

fn tutorial_list() -> Vec<Tutorial> {
    let assignment_tut = Tutorial {
        number: "1.1",
        title: "Assigning values to event fields",
        help_text: ASSIGNMENT_TEXT,
        correct_answer: Value::from(map!["severity": "info"]),
        initial_event: Value::from(map![]),
    };

    let deletion_tut = Tutorial {
        number: "1.2",
        title: "Deleting fields",
        help_text: DELETION_TEXT,
        correct_answer: Value::from(map!["three": 3]),
        initial_event: Value::from(map!["one": 1, "two": 2, "three": 3]),
    };

    let rename_tut = Tutorial {
        number: "1.3",
        title: "Renaming fields",
        help_text: RENAME_TEXT,
        correct_answer: Value::from(map!["new_field": "old value"]),
        initial_event: Value::from(map!["old_field": "old value"]),
    };

    vec![assignment_tut, deletion_tut, rename_tut]
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

TASK:
Assign the string `"info"` to the field `severity`.
"#;

const DELETION_TEXT: &str = r#"You can delete fields using the `del` function:

del(.field)

TASK:
Use the `del` function to get rid of the `one` and `two` fields.
"#;

const RENAME_TEXT: &str = r#"When you delete a field, the `del` function returns the value
of the deleted field. You can change the names of fields by assigning a new
field the value of the deleted field:

.new = del(.old)

TASK:
Use the `del` function to rename `old_field` to `new_field`.
"#;
