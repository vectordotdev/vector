use yew::prelude::*;

struct App;

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(_: Self::Properties, _: ComponentLink<Self>) -> Self {
        Self
    }

    fn update(&mut self, _: Self::Message) -> ShouldRender {
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
            </>
        }
    }
}

fn main() {
    yew::start_app::<App>();
}
