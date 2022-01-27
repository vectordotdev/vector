use vector::metrics::Controller;

fn prepare_metrics(cardinality: usize) -> &'static Controller {
    let _ = vector::metrics::init_test();
    let controller = Controller::get().unwrap();
    controller.reset();

    for idx in 0..cardinality {
        metrics::counter!("test", 1, "idx" => format!("{}", idx));
    }

    assert_eq!(controller.capture_metrics().len(), cardinality + 1);

    controller
}

#[test]
fn cardinality_matches() {
    for cardinality in &[0, 1, 10, 100, 1000, 10000] {
        let controller = prepare_metrics(*cardinality);
        let list = controller.capture_metrics();
        assert_eq!(list.len(), cardinality + 1);
    }
}
