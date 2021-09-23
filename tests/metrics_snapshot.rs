use vector::metrics::Controller;

fn prepare_metrics(cardinality: usize) -> Controller {
    let _ = vector::metrics::init_test();
    let controller = Controller::get().unwrap();
    controller.reset();

    for idx in 0..cardinality {
        metrics::counter!("test", 1, "idx" => format!("{}", idx));
    }

    assert_cardinality_matches(&controller.capture_metrics(), cardinality + 1);

    controller
}

fn assert_cardinality_matches(iter: &impl Iterator, cardinality: usize) {
    assert_eq!(iter.size_hint(), (cardinality, Some(cardinality)));
}

#[test]
fn cardinality_matches() {
    for cardinality in &[0, 1, 10, 100, 1000, 10000] {
        let controller = prepare_metrics(*cardinality);
        let iter = controller.capture_metrics();
        assert_cardinality_matches(&iter, *cardinality + 1);
    }
}
