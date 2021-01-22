use remap::{state, Formatter, Program, Runtime, Value};
use remap_functions::all as funcs;
use rustyline::{error::ReadlineError, Editor};
use super::{repl::Repl, Error};

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

    let tut1 = Tutorial {
        title: "Syslog",
        help_text: "TODO",
        object: Value::from(1),
    };

    let tut2 = Tutorial {
        title: "JSON",
        help_text: "This is the second tutorial",
        object: Value::from(2),
    };

    let tut3 = Tutorial {
        title: "Something else",
        help_text: "This is the third tutorial",
        object: Value::from(3),
    };

    let mut tutorials = vec![tut1, tut2, tut3];

    println!("Welcome to the Vector Remap Language interactive tutorial!\n");

    let mut index: usize = 0;

    print_help_text(index, &tutorials);

    loop {
        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "exit" || line == "quit" => break,
            Ok(line) => {
                rl.add_history_entry(line);

                match line {
                    "next" => {
                        if (index + 1) == tutorials.len() {
                            println!("You've finished all the steps! Taking you back to the beginning\n");
                            index = 0;
                            print_help_text(index, &tutorials);
                        } else {
                            index = index.saturating_add(1);
                            print_help_text(index, &tutorials);
                        }
                    }
                    "prev" => {
                        index = index.saturating_sub(1);
                        print_help_text(index, &tutorials);
                    }
                    "" => continue,
                    command => {
                        let object = &mut tutorials[index].object;
                        let value = run_tutorial(object, &mut rt, command, &mut compiler_state);
                        println!("{}\n", value);
                    },
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

fn print_help_text(index: usize, tutorials: &Vec<Tutorial>) {
    let tut = &tutorials[index];
    println!("Tutorial {}: {}\n\n{}\n\nObject: {}\n", index + 1, tut.title, tut.help_text, tut.object);
}

fn run_tutorial(
    object: &mut Value,
    runtime: &mut Runtime,
    program: &str,
    state: &mut state::Compiler,
) -> String {
    let program = match Program::new_with_state(
        program.to_owned(),
        &funcs(),
        None,
        true,
        state,
    ) {
        Ok((program, _)) => program,
        Err(diagnostics) => return Formatter::new(program, diagnostics).colored().to_string(),
    };

    match runtime.run(object, &program) {
        Ok(value) => value.to_string(),
        Err(err) => err.to_string(),
    }
}
