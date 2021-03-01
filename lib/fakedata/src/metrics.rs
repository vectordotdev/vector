use crate::random_n;

pub fn metric_lines(namespace: &str, names: &[String]) -> Vec<String> {
    names.iter().map(|name| metric_line(namespace, name)).collect()
}

fn metric_line(namespace: &str, name: &str) -> String {
    let n = random_n(0.0, 10.0);
    format!("{}_{} {}", namespace, name, n)
}
