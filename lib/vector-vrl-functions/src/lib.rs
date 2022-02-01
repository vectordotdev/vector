pub mod get_metadata_field;
pub mod remove_metadata_field;
pub mod set_metadata_field;

pub fn vrl_functions() -> Vec<Box<dyn vrl_core::Function>> {
    vec![
        Box::new(get_metadata_field::GetMetadataField) as _,
        Box::new(remove_metadata_field::RemoveMetadataField) as _,
        Box::new(set_metadata_field::SetMetadataField) as _,
    ]
}
