use vector_buffers::{migrations::migrate_disk_v1_to_disk_v2, BufferType};
use vector_core::{config::ComponentKey, event::EventArray};

use crate::config::Config;

pub(crate) async fn run_disk_v1_to_disk_v2_migration(
    config: &Config,
    sink_id: &str,
) -> Result<(), String> {
    let sink_key: ComponentKey = sink_id.into();
    let sink_buffer_config = match config.sinks.get(&sink_key) {
        Some(config) => Ok(config.buffer.clone()),
        None => Err(format!(
            "Sink '{}' does not exist in the given configuration",
            sink_id
        )),
    }?;

    // Look for a disk v1 buffer, otherwise, we have nothing to migrate.
    if !sink_buffer_config
        .stages
        .iter()
        .any(|stage| matches!(stage, BufferType::DiskV1 { .. }))
    {
        return Err(format!(
            "Sink '{}' is not configured to use a disk v1 buffer",
            sink_id
        ));
    }

    // Lastly, make sure a data directory is actually present.
    let data_dir = match config.global.data_dir.clone() {
        Some(data_dir) => Ok(data_dir),
        None => Err("No data directory is configured. Disk buffers cannot exist/be used without a data directory".to_string()),
    }?;

    // We do some simple checks here but ultimately we defer the logic to the `vector_buffers` crate
    // so that it doesn't have to leak a bunch of implementation details.
    migrate_disk_v1_to_disk_v2::<EventArray>(data_dir, sink_id.to_string()).await
}
