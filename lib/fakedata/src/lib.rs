use rand::{thread_rng, Rng};

pub mod logs;
pub mod metrics;

// Helper functions
fn random_in_range(min: usize, max: usize) -> String {
    thread_rng().gen_range(min..max).to_string()
}

fn random_n(min: f64, max: f64) -> String {
    thread_rng().gen_range(min..max).to_string()
}

fn random_from_array<T: Copy>(v: &[T]) -> T {
    v[thread_rng().gen_range(0..v.len())]
}