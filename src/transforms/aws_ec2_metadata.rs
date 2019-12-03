use super::Transform;
use hyper::{Client, Body, Uri};
use futures03::compat::Future01CompatExt;
use crate::{
    event::Event,
    topology::config::{TransformConfig, DataType, TransformDescription},
    runtime::TaskExecutor,
};
use serde::{Serialize, Deserialize};

async fn get_metadata() -> Result<(), crate::Error> {
    let mut client = Client::new();

    let res = client.get(Uri::from_static("http://127.0.0.1:5555/latest/meta-data")).compat().await?;
    println!("res {:?}", res);    

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ec2Metadata {}

inventory::submit! {
    TransformDescription::new_without_default::<Ec2Metadata>("aws_ec2_metadata")
}

#[typetag::serde(name = "aws_ec2_metadata")]
impl TransformConfig for Ec2Metadata {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        unimplemented!()
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "add_tags"
    }
}

impl Transform for Ec2Metadata {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        unimplemented!()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMetadata {
    private_ip: String,
    pub availability_zone: String,
    pub instance_id: String,
    pub instance_type: String,
    pub account_id: String,
    architecture: String, // FIXME: best data type?
    pub image_id: String,
    region: String, 
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures03::future::{FutureExt, TryFutureExt};

    #[test]
    fn fetch() {
        let mut rt = crate::runtime::Runtime::single_threaded().unwrap();

        rt.block_on_std(get_metadata()).unwrap();
    }
}