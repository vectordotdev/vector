use rand::{thread_rng, Rng};

pub mod logs;

// Helper functions
fn random_in_range(min: usize, max: usize) -> String {
    thread_rng().gen_range(min..max).to_string()
}

fn random_from_array<T: Copy>(v: &[T]) -> T {
    v[thread_rng().gen_range(0..v.len())]
}

// For generating random counters
pub fn random_counter(min: usize, max: usize) -> f64 {
    thread_rng().gen_range(min..max) as f64
}
