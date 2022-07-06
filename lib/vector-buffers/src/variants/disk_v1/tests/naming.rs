use crate::variants::disk_v1::{
    get_new_style_buffer_dir_name, get_old_style_buffer_dir_name,
    get_sidelined_old_style_buffer_dir_name,
};

#[test]
fn buffer_dir_names() {
    // I realize that this test might seem silly -- we're just checking that it generates a
    // string in a certain way -- but ironically, this test existing prior to #10379 may have
    // saved us from needing the wall of code that's prresent at the top of the file.
    //
    // Here, we're simply testing that the "old" style name is suffixed with `_buffer` and that
    // the "new" style name is suffxed with `_id` to match the current behavior.  To ensure that
    // what we're testing is actually what's being used to generate buffer directory names,
    // we've slightly refactored the aforementioned wall of code to use these functions.
    let old_result = get_old_style_buffer_dir_name("foo");
    let new_result = get_new_style_buffer_dir_name("foo");
    let sidelined_old_result = get_sidelined_old_style_buffer_dir_name("foo");

    assert_eq!("foo_buffer", old_result);
    assert_eq!("foo_id", new_result);
    assert_eq!("foo_buffer_old", sidelined_old_result);
    assert_ne!(old_result, new_result);
    assert_ne!(new_result, sidelined_old_result);
    assert_ne!(old_result, sidelined_old_result);
}
