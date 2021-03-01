use crate::random_n;

pub fn counter(namespace: String, name: String) -> String {
    let n = random_n(1.0, 25.0);

    format!("{}_{}_{}", namespace, name, n)
}