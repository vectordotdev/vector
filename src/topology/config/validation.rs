use crate::topology::Config;
use std::collections::HashSet;

// Modified version of Kahn's topological sort algorithm that ignores the actual sorted output and
// only cares if the sort was possible (i.e. whether or not there was a cycle in the input graph).
pub fn contains_cycle(config: &Config) -> bool {
    let nodes = config
        .sources
        .keys()
        .chain(config.transforms.keys())
        .chain(config.sinks.keys())
        .collect::<HashSet<_>>();

    let mut edges = HashSet::new();
    for (name, transform) in config.transforms.iter() {
        for input in transform.inputs.iter() {
            if nodes.contains(input) {
                edges.insert((input, name));
            }
        }
    }
    for (name, sink) in config.sinks.iter() {
        for input in sink.inputs.iter() {
            if nodes.contains(input) {
                edges.insert((input, name));
            }
        }
    }

    let mut no_incoming = nodes
        .into_iter()
        .filter(|n| !edges.iter().any(|(_t, h)| h == n))
        .collect::<Vec<_>>();
    while let Some(node) = no_incoming.pop() {
        let outgoing = edges
            .clone()
            .into_iter()
            .filter(|(tail, _head)| tail == &node)
            .collect::<Vec<_>>();
        for edge in outgoing {
            edges.remove(&edge);
            let successor = edge.1;
            if edges.iter().filter(|(_t, head)| head == &successor).count() == 0 {
                no_incoming.push(successor);
            }
        }
    }
    !edges.is_empty()
}

#[cfg(test)]
mod test {
    use super::contains_cycle;
    use crate::topology::Config;

    #[test]
    fn detects_cycles() {
        let cyclic = Config::load(
            r#"
            [sources.in]
            type = "tcp"
            address = "127.0.0.1:1235"

            [transforms.one]
            type = "sampler"
            inputs = ["in", "three"]
            rate = 10
            pass_list = []

            [transforms.two]
            type = "sampler"
            inputs = ["one"]
            rate = 10
            pass_list = []

            [transforms.three]
            type = "sampler"
            inputs = ["two"]
            rate = 10
            pass_list = []

            [sinks.out]
            type = "tcp"
            inputs = ["three"]
            address = "127.0.0.1:9999"
          "#
            .as_bytes(),
        )
        .unwrap();

        assert_eq!(true, contains_cycle(&cyclic));
    }

    #[test]
    fn doesnt_detect_noncycles() {
        let acyclic = Config::load(
            r#"
            [sources.in]
            type = "tcp"
            address = "127.0.0.1:1235"

            [transforms.one]
            type = "sampler"
            inputs = ["in"]
            rate = 10
            pass_list = []

            [transforms.two]
            type = "sampler"
            inputs = ["in"]
            rate = 10
            pass_list = []

            [transforms.three]
            type = "sampler"
            inputs = ["one", "two"]
            rate = 10
            pass_list = []

            [sinks.out]
            type = "tcp"
            inputs = ["three"]
            address = "127.0.0.1:9999"
          "#
            .as_bytes(),
        )
        .unwrap();

        assert_eq!(false, contains_cycle(&acyclic));
    }
}
