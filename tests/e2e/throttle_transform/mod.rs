use std::{
    fs,
    path::Path,
    time::Duration,
};

/// Read an output file from the shared volume, retrying until it exists.
fn read_output_file(filename: &str) -> String {
    let path = Path::new("/output").join(filename);
    let mut attempts = 0;
    loop {
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap();
            if !content.is_empty() {
                return content;
            }
        }
        attempts += 1;
        if attempts > 60 {
            panic!("Output file {} not found after 30 seconds", path.display());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Count JSON lines in output content.
fn count_json_events(content: &str) -> usize {
    content.lines().filter(|l| !l.trim().is_empty()).count()
}

#[test]
fn throttle_events_flow_through() {
    let primary = read_output_file("primary.log");
    let primary_count = count_json_events(&primary);

    // With 200 events across 5 services and threshold=20 per service,
    // we expect up to 100 events (20 per service × 5 services)
    assert!(
        primary_count > 0,
        "Expected some events to pass, got none"
    );
    assert!(
        primary_count <= 100,
        "Expected at most 100 events (20 per key × 5 keys), got {primary_count}"
    );
}

#[test]
fn throttle_dropped_events_routed() {
    // Only applicable for vector_bytes.yaml and vector_multi.yaml configs
    let dropped_path = Path::new("/output/dropped.log");
    if !dropped_path.exists() {
        // Skip for configs without reroute_dropped
        return;
    }

    let dropped = fs::read_to_string(dropped_path).unwrap();
    let dropped_count = count_json_events(&dropped);

    let primary = read_output_file("primary.log");
    let primary_count = count_json_events(&primary);

    // All events should end up in either primary or dropped
    let total = primary_count + dropped_count;
    assert!(
        total > 0,
        "Expected some events in primary + dropped, got none"
    );

    // With any throttle limit, some events should be dropped
    assert!(
        dropped_count > 0,
        "Expected some events in dropped output, got none"
    );
}
