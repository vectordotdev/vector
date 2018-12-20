use flate2::read::GzDecoder;
use futures::{stream, Future, Sink};
use router::test_util::{random_lines, random_string};
use router::{sinks, Record};
use rusoto_core::region::Region;
use rusoto_s3::{S3Client, S3};
use std::io::{BufRead, BufReader};

const ACCESS_KEY: &str = "DUMW7A4DJNEIX5QSNW8Y";
const SECRET_KEY: &str = "JxJx+Vt3pCnd3yBRkGyqRlIB3PMyuywAEWHgXfb+";
const BUCKET: &str = "router-tests";

#[cfg_attr(not(feature = "s3-integration-tests"), ignore)]
#[test]
fn test_insert_message_into_s3() {
    ensure_bucket(&client());

    let prefix = random_string(10) + "/";
    let sink = sinks::s3::new(
        client(),
        prefix.clone(),
        2 * 1024 * 1024,
        BUCKET.to_string(),
        false,
    );

    let lines = random_lines(100).take(10).collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.and_then(|sink| sink.send_all(stream::iter_ok(records.into_iter())));

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let (mut sink, _) = rt.block_on(pump).unwrap();
    rt.block_on(futures::future::poll_fn(move || sink.close()))
        .unwrap();

    let list_res = client()
        .list_objects_v2(rusoto_s3::ListObjectsV2Request {
            bucket: BUCKET.to_string(),
            prefix: Some(prefix),
            ..Default::default()
        })
        .sync()
        .unwrap();

    println!("{:?}", list_res);

    let keys = list_res
        .contents
        .unwrap()
        .into_iter()
        .map(|obj| obj.key.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 1);

    let key = keys[0].clone();
    assert!(key.ends_with(".log"));

    let obj = client()
        .get_object(rusoto_s3::GetObjectRequest {
            bucket: BUCKET.to_string(),
            key: key,
            ..Default::default()
        })
        .sync()
        .unwrap();

    assert_eq!(obj.content_encoding, None);

    let response_lines = {
        let buf_read = BufReader::new(obj.body.unwrap().into_blocking_read());
        buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
    };

    assert_eq!(lines, response_lines);
}

#[cfg_attr(not(feature = "s3-integration-tests"), ignore)]
#[test]
fn test_rotate_files_after_the_buffer_size_is_reached() {
    ensure_bucket(&client());

    let prefix = random_string(10) + "/";
    let sink = sinks::s3::new(client(), prefix.clone(), 1000, BUCKET.to_string(), false);

    let lines = random_lines(100).take(30).collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.and_then(|sink| sink.send_all(stream::iter_ok(records.into_iter())));

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let (mut sink, _) = rt.block_on(pump).unwrap();
    rt.block_on(futures::future::poll_fn(move || sink.close()))
        .unwrap();

    let list_res = client()
        .list_objects_v2(rusoto_s3::ListObjectsV2Request {
            bucket: BUCKET.to_string(),
            prefix: Some(prefix),
            ..Default::default()
        })
        .sync()
        .unwrap();

    let keys = list_res
        .contents
        .unwrap()
        .into_iter()
        .map(|obj| obj.key.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 3);

    let response_lines = keys
        .into_iter()
        .map(|key| {
            let obj = client()
                .get_object(rusoto_s3::GetObjectRequest {
                    bucket: BUCKET.to_string(),
                    key: key,
                    ..Default::default()
                })
                .sync()
                .unwrap();

            let response_lines = {
                let buf_read = BufReader::new(obj.body.unwrap().into_blocking_read());
                buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
            };

            response_lines
        })
        .collect::<Vec<_>>();

    assert_eq!(&lines[00..10], response_lines[0].as_slice());
    assert_eq!(&lines[10..20], response_lines[1].as_slice());
    assert_eq!(&lines[20..30], response_lines[2].as_slice());
}

#[cfg_attr(not(feature = "s3-integration-tests"), ignore)]
#[test]
fn test_gzip() {
    ensure_bucket(&client());

    let prefix = random_string(10) + "/";
    let sink = sinks::s3::new(client(), prefix.clone(), 1000, BUCKET.to_string(), true);

    let lines = random_lines(100).take(500).collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| Record::new_from_line(line.clone()))
        .collect::<Vec<_>>();

    let pump = sink.and_then(|sink| sink.send_all(stream::iter_ok(records.into_iter())));

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let (mut sink, _) = rt.block_on(pump).unwrap();
    rt.block_on(futures::future::poll_fn(move || sink.close()))
        .unwrap();

    let list_res = client()
        .list_objects_v2(rusoto_s3::ListObjectsV2Request {
            bucket: BUCKET.to_string(),
            prefix: Some(prefix),
            ..Default::default()
        })
        .sync()
        .unwrap();

    let keys = list_res
        .contents
        .unwrap()
        .into_iter()
        .map(|obj| obj.key.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 2);

    let response_lines = keys
        .into_iter()
        .map(|key| {
            assert!(key.ends_with(".log.gz"));

            let obj = client()
                .get_object(rusoto_s3::GetObjectRequest {
                    bucket: BUCKET.to_string(),
                    key: key,
                    ..Default::default()
                })
                .sync()
                .unwrap();

            assert_eq!(obj.content_encoding, Some("gzip".to_string()));

            let response_lines = {
                let buf_read =
                    BufReader::new(GzDecoder::new(obj.body.unwrap().into_blocking_read()));
                buf_read.lines().map(|l| l.unwrap()).collect::<Vec<_>>()
            };

            response_lines
        })
        .flatten()
        .collect::<Vec<_>>();

    assert_eq!(lines, response_lines);
}

fn client() -> S3Client {
    let region = Region::Custom {
        name: "minio".to_owned(),
        endpoint: "http://localhost:9000".to_owned(),
    };

    let provider = rusoto_credential::StaticProvider::new_minimal(
        ACCESS_KEY.to_string(),
        SECRET_KEY.to_string(),
    );
    let dispatched = rusoto_core::request::HttpClient::new().unwrap();
    S3Client::new_with(dispatched, provider, region)
}

fn ensure_bucket(client: &S3Client) {
    use rusoto_s3::{CreateBucketError, CreateBucketRequest};

    let req = CreateBucketRequest {
        bucket: BUCKET.to_string(),
        ..Default::default()
    };

    let res = client.create_bucket(req);

    match res.sync() {
        Ok(_) | Err(CreateBucketError::BucketAlreadyOwnedByYou(_)) => {}
        e => {
            panic!("Couldn't create bucket: {:?}", e);
        }
    }
}
