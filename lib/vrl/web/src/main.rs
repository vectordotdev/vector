#![recursion_limit = "1024"]

use std::collections::BTreeMap;
use vrl::{diagnostic::Formatter, state, Runtime, Target, Value};
use yew::events::KeyboardEvent;
use yew::prelude::*;
use yew::services::console::ConsoleService;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("json parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("program parsing error: {0}")]
    Parse(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}

struct State {
    program: String,
    output: String,
    value: Value,
}

struct App {
    link: ComponentLink<Self>,
    state: State,
}

enum Action {
    Update(String),
    Compile,
}

fn log(msg: &str) {
    ConsoleService::info(msg)
}

fn log_s(s: String) {
    ConsoleService::info(&s)
}

impl Component for App {
    type Message = Action;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let program = String::new();
        let output = String::new();

        let mut m: BTreeMap<String, Value> = BTreeMap::new();
        m.insert("foo".into(), "bar".into());
        let value = Value::Object(m);

        let state = State {
            program,
            output,
            value,
        };

        Self { link, state }
    }

    fn update(&mut self, action: Self::Message) -> ShouldRender {
        use Action::*;

        log("Updated...");

        match action {
            Update(program) => {
                self.state.program = program;
            }
            Compile => {
                log(&format!("Current program: {}", self.state.program));

                match parse_input(&mut self.state.value, &self.state.program) {
                    Ok(res) => {
                        log_s(format!("OK: {}", res));
                    }
                    Err(err) => {
                        log_s(format!("Error: {}", err));
                    }
                }
            }
        }

        true
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        html! {
            <>
                <nav class="navbar is-black" role="navigation">
                    <div class="container">
                        <div class="navbar-brand">
                            <a class="navbar-item has-text-primary has-text-weight-bold" href="https://vrl.dev">
                                { "Vector Remap Language" }
                            </a>
                        </div>
                    </div>
                </nav>

                <section class="section">
                    <div class="container">
                        <div class="columns is-multiline is-8">
                            <div class="column">
                                <p class="is-size-2">
                                    { "Program" }
                                </p>

                                <br />

                                {self.view_input()}
                            </div>

                            <div class="column">
                                <p class="is-size-2">
                                    { "Output" }
                                </p>

                                {self.compiled_output()}
                            </div>
                        </div>
                    </div>

                    <br />
                    <br />

                    <div class="container">
                        <p>
                            {self.state.value.clone()}
                        </p>
                    </div>
                </section>
            </>
        }
    }
}

impl App {
    fn view_input(&self) -> Html {
        html! {
            <div class="control">
                <input
                    type="text"
                    class="input"
                    value=&self.state.program
                    oninput=self.link.callback(|input: InputData| {
                        Action::Update(input.value)
                    })
                    onkeypress=self.link.batch_callback(move |e: KeyboardEvent| {
                        if e.key() == "Enter" { vec![Action::Compile] } else { vec![] }
                    })
                />
            </div>
        }
    }

    fn compiled_output(&self) -> Html {
        html! {
            <p>
                {&self.state.output}
            </p>
        }
    }
}

fn parse_input(object: &mut impl Target, source: &str) -> Result<Value, Error> {
    let state = state::Runtime::default();
    let mut runtime = Runtime::new(state);
    let program = vrl::compile(&source, &stdlib::all()).map_err(|diagnostics| {
        Error::Parse(Formatter::new(&source, diagnostics).colored().to_string())
    })?;

    runtime
        .resolve(object, &program)
        .map_err(|err| Error::Runtime(err.to_string()))
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::start_app::<App>();
}
