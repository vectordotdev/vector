#![recursion_limit = "1024"]

use std::collections::BTreeMap;
use std::convert::Into;
use vrl::{diagnostic::Formatter, state::{Compiler as CompilerState, Runtime as StateRuntime}, Runtime, Target, Value};
use yew::events::KeyboardEvent;
use yew::prelude::*;
use yew::services::console::ConsoleService;

#[derive(Debug, thiserror::Error)]
enum Error {}

struct AppState {
    vrl_program: String,
    output: String,
    current_value: Value,
}

struct App {
    link: ComponentLink<Self>,
    app_state: AppState,
    processor: Processor,
    compiler_state: CompilerState,
}


struct Processor {
    runtime: Runtime,
}

impl Processor {
    fn new() -> Self {
        let state = StateRuntime::default();
        let runtime = Runtime::new(state);

        Self { runtime }
    }

    fn parse_input(&mut self, object: &mut impl Target, program: &str, state: &mut CompilerState) -> String {
        let program = match vrl::compile_with_state(program, &stdlib::all(), state) {
            Ok(program) => program,
            Err(diagnostics) => return Formatter::new(program, diagnostics).colored().to_string(),
        };

        let runtime = &mut self.runtime;

        match runtime.resolve(object, &program) {
            Ok(obj) => obj.to_string(),
            Err(err) => err.to_string()
        }
    }
}

enum Action {
    Update(String),
    Compile,
}

fn log(msg: &str) {
    ConsoleService::info(msg)
}

impl Component for App {
    type Message = Action;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let initial_program = "".to_owned();
        let output = "".to_owned();
        let compiler_state = CompilerState::default();
        let initial_value = Self::initial_object();

        let app_state = AppState {
            vrl_program: initial_program,
            output,
            current_value: initial_value,
        };

        let processor = Processor::new();

        Self { link, app_state, processor, compiler_state }
    }

    fn update(&mut self, action: Self::Message) -> ShouldRender {
        use Action::*;

        match action {
            Update(program) => {
                self.app_state.vrl_program = program;
            }
            Compile => {
                log(&format!("Current program: {}", self.app_state.vrl_program));

                let result = self.processor.parse_input(&mut self.app_state.current_value, &self.app_state.vrl_program, &mut self.compiler_state);

                self.app_state.output = result;
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
                <div class="page">
                    <main class="main">
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
                                <div class="card">
                                    <div class="card-content">
                                        <div class="columns is-multiline is-8">
                                            <div class="column">
                                                <p class="title">
                                                    { "Program" }
                                                </p>

                                                {self.view_input()}

                                                {self.vrl_output()}
                                            </div>

                                            <div class="column">
                                                {self.current_object()}
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </section>
                    </main>

                    <footer class="footer">
                        <div class="container">
                            <p>
                                { "Brought to you by the " }
                                <a href="https://vector.dev" target="_blank">{ "Vector" }</a>
                                { " team" }
                            </p>
                        </div>
                    </footer>
                </div>
            </>
        }
    }
}

impl App {
    fn initial_object() -> Value {
        let mut m: BTreeMap<String, Value> = BTreeMap::new();
        let mut msg: BTreeMap<String, Value> = BTreeMap::new();
        msg.insert("foo".into(), "bar".into());
        m.insert("message".into(), Value::Object(msg));
        m.insert("timestamp".into(), "2021-03-02T19:25:05.205732Z".into());
        Value::Object(m)
    }

    fn view_input(&self) -> Html {
        html! {
            <div class="control">
                <input
                    type="text"
                    class="input"
                    value=&self.app_state.vrl_program
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

    fn vrl_output(&self) -> Html {
        if &self.app_state.output != "" {
            html! {
                <>
                    <br /><br />

                    <div class="card">
                        <div class="card-content">
                            <p class="is-size-4">
                                { "Output" }
                            </p>



                            <p class="console-output">
                                {&self.app_state.output}
                            </p>
                        </div>
                    </div>
                </>
            }
        } else {
            html! {}
        }
    }

    fn current_object(&self) -> Html {
        let value_as_string = self.app_state.current_value.to_string();

        html! {
            <>
                <p class="title">
                    { "Current event value" }
                </p>

                <p class="console-output">
                    {value_as_string}
                </p>
            </>
        }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::start_app::<App>();
}
