use vector_integration_managers::*;

mod core;

fn main() -> Result<()> {
    docker_main(core::start, core::stop)
}
