pub mod get_metadata_field;
pub mod remove_metadata_field;
pub mod set_metadata_field;

pub fn vrl_functions() -> Vec<Box<dyn vrl_core::Function>> {
    vec![
        Box::new(get_metadata_field::GetMetadataField) as Box<dyn vrl_core::Function>,
        Box::new(remove_metadata_field::RemoveMetadataField) as Box<dyn vrl_core::Function>,
        Box::new(set_metadata_field::SetMetadataField) as Box<dyn vrl_core::Function>,
    ]
}
