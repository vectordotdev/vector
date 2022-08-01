#[test]
fn test_basic_functionality() {
    let library = performance_counters::Library::new().unwrap();
    let counting = library.start_counting().unwrap();

    let start = counting.get_counters();
    let mut vec = Vec::new();

    for i in 0..2000 {
        let mut new = vec.clone();
        new.push(i);
        vec = new;
    }

    let end = counting.get_counters();

    let result = end - start;

    assert!(result.cycles > 0);
    assert!(result.load_store_instructions > 0);
    assert!(result.l1_data_load_cache_misses > 0);
    assert!(result.l1_data_store_cache_misses > 0);

    drop(counting);
}
