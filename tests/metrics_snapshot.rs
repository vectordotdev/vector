fn prepare_metrics(cardinality: usize) -> &'static vector::metrics::Controller {
    let _ = vector::metrics::init();
    let controller = vector::metrics::get_controller().unwrap();
    vector::metrics::reset(controller);

    for idx in 0..cardinality {
        metrics::counter!("test", 1, "idx" => format!("{}", idx));
    }

    assert_cardinality_matches(
        &vector::metrics::capture_metrics(controller),
        cardinality + 1,
    );

    controller
}

fn assert_cardinality_matches(iter: &impl Iterator, cardinality: usize) {
    assert_eq!(iter.size_hint(), (cardinality, Some(cardinality)));
}

#[test]
fn cardinality_matches() {
    for cardinality in &[0, 1, 10, 100, 1000, 10000] {
        let controller = prepare_metrics(*cardinality);
        let iter = vector::metrics::capture_metrics(controller);
        assert_cardinality_matches(&iter, *cardinality + 1);
    }
}
