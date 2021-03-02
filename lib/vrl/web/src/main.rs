#![recursion_limit="1024"]

use yew::events::KeyboardEvent;
use yew::prelude::*;
use yew::services::console::ConsoleService;

struct State {
    program: String,
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

impl Component for App {
    type Message = Action;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let program = String::new();
        let state = State { program };

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

                                <br />
                            </div>
                        </div>
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
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::start_app::<App>();
}
