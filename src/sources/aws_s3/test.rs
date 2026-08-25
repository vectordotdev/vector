use tokio::io::AsyncReadExt;

use super::*;

#[test]
fn request_payer_config() {
    let config: AwsS3Config = serde_yaml::from_str("{}").unwrap();
    assert_eq!(config.request_payer, None);

    let config: AwsS3Config = serde_yaml::from_str("request_payer: requester").unwrap();
    assert_eq!(config.request_payer, Some(S3RequestPayer::Requester));

    assert!(serde_yaml::from_str::<AwsS3Config>("request_payer: bucket_owner").is_err());
}

#[test]
fn request_payer_converts_to_aws_sdk_type() {
    assert_eq!(
        RequestPayer::from(S3RequestPayer::Requester),
        RequestPayer::Requester
    );
}

#[test]
fn determine_compression() {
    use super::Compression;

    let cases = vec![
        ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
        (
            "out.log",
            None,
            Some("application/gzip"),
            Some(Compression::Gzip),
        ),
        ("out.log.gz", None, None, Some(Compression::Gzip)),
        ("out.txt", None, None, None),
    ];
    for case in cases {
        let (key, content_encoding, content_type, expected) = case;
        assert_eq!(
            super::determine_compression(content_encoding, content_type, key),
            expected,
            "key={key:?} content_encoding={content_encoding:?} content_type={content_type:?}",
        );
    }
}

#[tokio::test]
async fn decode_empty_message_gzip() {
    let key = uuid::Uuid::new_v4().to_string();

    let mut data = Vec::new();
    s3_object_decoder(
        Compression::Auto,
        &key,
        Some("gzip"),
        None,
        ByteStream::default(),
    )
    .await
    .read_to_end(&mut data)
    .await
    .unwrap();

    assert!(data.is_empty());
}
