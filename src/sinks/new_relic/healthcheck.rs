use crate::{
    http::HttpClient,
    sinks::HealthcheckError
};
use serde::{
    Deserialize, Serialize
};
use http::{
    Request, StatusCode, Uri
};
use super::{
    NewRelicApi, NewRelicRegion,
};

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusModel {
    page: NewRelicStatusPage,
    components: Vec<NewRelicStatusComponent>,
}

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusPage {
    id: String,
    name: String,
    url: String
}

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusComponent {
    id: String,
    name: String,
    status: String
}

pub async fn healthcheck(
    client: HttpClient,
    api: NewRelicApi,
    region: NewRelicRegion
) -> crate::Result<()> {

    let status_uri = Uri::from_static("https://status.newrelic.com/api/v2/components.json");
    let request = Request::get(status_uri)
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => {},
        other => return Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }

    let body_bytes = hyper::body::to_bytes(response.into_body()).await?;
    let status_model: NewRelicStatusModel = serde_json::from_slice(&body_bytes).unwrap();

    let component_name = match api {
        NewRelicApi::Events => {
            match region {
                NewRelicRegion::Us => "Event API : US".to_owned(),
                NewRelicRegion::Eu => "Event API : Europe".to_owned()
            }
        },
        NewRelicApi::Metrics => {
            match region {
                NewRelicRegion::Us => "Metric API : US".to_owned(),
                NewRelicRegion::Eu => "Metric API : Europe".to_owned()
            }
        },
        NewRelicApi::Logs => {
            match region {
                NewRelicRegion::Us => "Log API : US".to_owned(),
                NewRelicRegion::Eu => "Log API : Europe".to_owned()
            }
        }
    };
    
    let component = status_model.components
        .iter()
        .find(|component| component.name == component_name);

    if let Some(component) = component {
        if component.status == "operational" {
            Ok(())
        }
        else {
            Err(HealthcheckError::UnexpectedStatus { status:  StatusCode::SERVICE_UNAVAILABLE}.into())
        }
    }
    else {
        Err(HealthcheckError::UnexpectedStatus { status:  StatusCode::NOT_FOUND}.into())
    }
}
