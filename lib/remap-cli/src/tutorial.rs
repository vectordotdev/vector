use super::{Error, Repl};
use remap::{state, Formatter, Object, Program, Runtime, Value};
use remap_functions::all as funcs;
use rustyline::{error::ReadlineError, Editor};
use serde::Deserialize;
use std::fs::File;
use std::io::Read;

const TUTORIALS_TOML_FILE: &str = "./tutorials.toml";

#[derive(Deserialize)]
struct Tutorial {
    number: String, // Making this a string allows for 1.1, 2.5, etc.
    title: String,
    help_text: String,
    correct_answer: Value,
    initial_event: Value,
}

#[derive(Deserialize)]
struct Tutorials {
    tutorials: Vec<Tutorial>,
}

pub fn tutorial() -> Result<(), Error> {
    let mut index = 0;
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new("> ")));

    let mut tutorials = load_tutorials_from_json()?.tutorials;

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
                            println!("\n\nCongratulations! You've successfully completed the VRL tutorial.\n");
                            break;
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
                        match resolve_to_value(event, &mut rt, command, &mut compiler_state) {
                            Ok(result) => {
                                if event == &tut.correct_answer {
                                    println!("\n\nCORRECT! You have wisely ended up with this event:\n{}\n", event);

                                    if (index + 1) == tutorials.len() {
                                        println!("\n\nCongratulations! You've successfully completed the VRL tutorial.\n");
                                        break;
                                    } else {
                                        println!("You've completed tutorial {} out of {}\n", index + 1, tutorials.len());
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

fn load_tutorials_from_json() -> Result<Tutorials, Error> {
    let mut buf = String::new();
    let _ = File::open(TUTORIALS_TOML_FILE)?.read_to_string(&mut buf)?;

    match toml::from_str(&buf) {
        Ok(tuts) => Ok(tuts),
        Err(err) => Err(Error::Toml(err))
    }
}

// This function reworks the resolve function in repl.rs to return a Result rather than a String. If the Result is
// Ok, the value is used to check whether the current event is equal to the "correct" answer.
pub fn resolve_to_value(
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

// Help text
const HELP_TEXT: &str = r#"
Tutorial commands:
  next     Load the next tutorial
  prev     Load the previous tutorial
  exit     Exit the VRL interactive tutorial
"#;
