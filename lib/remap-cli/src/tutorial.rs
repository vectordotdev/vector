use remap::{state, Runtime, Value};
use rustyline::{error::ReadlineError, Editor};
use super::{repl::{resolve, Repl}, Error};

struct Tutorial {
    help_text: &'static str,
    object: Value,
}

pub fn tutorial() -> Result<(), Error> {
    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    let tut1 = Tutorial {
        help_text: "This is the first tutorial",
        object: Value::from(47),
    };

    let tutorials = vec![tut1];

    println!("Welcome to the Vector Remap Language interactive tutorial!\n");

    loop {
        let mut index: usize = 0;
        let mut objects: Vec<Value> = Vec::new();

        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "exit" || line == "quit" => break,
            Ok(line) => {
                rl.add_history_entry(line);

                let command = match line {
                    "next" => {
                        println!("next");
                        "."
                    }
                    "prev" => {
                        println!("prev");
                        "."
                    }
                    "" => continue,
                    _ => line,
                };

                let value = resolve(
                    objects.get_mut(index),
                    &mut rt,
                    command,
                    &mut compiler_state,
                );
                println!("{}\n", value);
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
