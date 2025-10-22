use crate::sources::odbc::client::{OdbcConfig, execute_query};
use crate::test_util::components::SOURCE_TAGS;
use crate::test_util::components::run_and_assert_source_compliance;
use bytes::Bytes;
use chrono::TimeZone;
use chrono_tz::Tz;
use odbc_api::ConnectionOptions;
use ordered_float::NotNan;
use std::borrow::Cow;
use std::fs;
use std::time::Duration;
use vector_lib::event::Event;
use vrl::prelude::*;
use vrl::value::Value;

enum DbType {
    MariaDb,
    Postgres,
}

fn get_db_type() -> DbType {
    match std::env::var("ODBC_DB_TYPE").as_deref() {
        Ok("mariadb") => DbType::MariaDb,
        Ok("postgresql") => DbType::Postgres,
        _ => panic!("Required environment variable 'ODBC_DB_TYPE'"),
    }
}

fn get_conn_str() -> String {
    std::env::var("ODBC_CONN_STRING").expect("Required environment variable 'ODBC_CONN_STRING'")
}

const fn get_conn_opt() -> ConnectionOptions {
    ConnectionOptions {
        login_timeout_sec: Some(3),
        packet_size: None,
    }
}

fn get_value_from_event<'a>(event: &'a Event, key: &str) -> Option<Cow<'a, str>> {
    let log = event.as_log();
    let msg = log.get_message()?;
    let arr_msg = msg.as_array_unwrap();
    let value = arr_msg[0].get(key);
    value?.as_str()
}

#[tokio::test]
async fn parse_odbc_config() {
    let conn_str = get_conn_str();
    let config_str = format!(
        r#"
            connection_string = "{conn_str}"
            statement = "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
            schedule = "*/5 * * * * *"
            schedule_timezone = "UTC"
            last_run_metadata_path = "odbc_tracking.json"
            tracking_columns = ["id", "name", "datetime"]
            statement_init_params = {{ id = "0", name = "test" }}
            iterations = 1
        "#
    );
    let config = toml::from_str::<OdbcConfig>(&config_str);
    assert!(
        config.is_ok(),
        "Failed to parse config: {}",
        config.unwrap_err()
    );
}

#[tokio::test]
async fn scheduled_query_executed() {
    let conn_str = get_conn_str();
    run_and_assert_source_compliance(
        OdbcConfig {
            connection_string: conn_str,
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT 1".to_string()),
            iterations: Some(1),
            ..Default::default()
        },
        Duration::from_secs(3),
        &SOURCE_TAGS,
    )
    .await;
}

#[tokio::test]
async fn query_executed_with_init_params() {
    const LAST_RUN_METADATA_PATH: &str = "odbc_tracking-integration-tests.json";

    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, get_conn_opt())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS odbc_table;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            match get_db_type() {
                DbType::MariaDb => {
                    r#"
CREATE TABLE odbc_table
(
    id int auto_increment primary key,
    name varchar(255) null,
    datetime datetime null
);
    "#
                }
                DbType::Postgres => {
                    r#"
CREATE TABLE odbc_table
(
    id SERIAL PRIMARY KEY,
    name VARCHAR(255),
    "datetime" TIMESTAMP NULL
);
"#
                }
            },
            (),
            Some(3),
        )
        .unwrap();
    let _ = conn
        .execute(
            r#"
INSERT INTO odbc_table (name, datetime) VALUES
('test1', now()),
('test2', now()),
('test3', now()),
('test4', now()),
('test5', now());
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let params = ObjectMap::from([("id".into(), 0.into())]);

    let _ = fs::remove_file(LAST_RUN_METADATA_PATH);

    let events = run_and_assert_source_compliance(
        OdbcConfig {
            connection_string: conn_str,
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT * FROM odbc_table WHERE id > ? LIMIT 1;".to_string()),
            statement_init_params: Some(params),
            tracking_columns: Some(vec!["id".to_string()]),
            last_run_metadata_path: Some(LAST_RUN_METADATA_PATH.to_string()),
            iterations: Some(5),
            ..Default::default()
        },
        Duration::from_secs(10),
        &SOURCE_TAGS,
    )
    .await;

    println!("{}", serde_json::to_string_pretty(&events).unwrap());
    assert_eq!(
        get_value_from_event(&events[0], "name"),
        Some("test1".into())
    );
    assert_eq!(
        get_value_from_event(&events[1], "name"),
        Some("test2".into())
    );
    assert_eq!(
        get_value_from_event(&events[2], "name"),
        Some("test3".into())
    );
    assert_eq!(
        get_value_from_event(&events[3], "name"),
        Some("test4".into())
    );
    assert_eq!(
        get_value_from_event(&events[4], "name"),
        Some("test5".into())
    );
}

#[tokio::test]
async fn query_executed_with_filepath() {
    const CONNECTION_STRING_FILE_PATH: &str = "odbc_connection_string.txt";
    const STATEMENT_FILE_PATH: &str = "odbc_statement.sql";
    const LAST_RUN_METADATA_PATH: &str = "odbc_tracking-integration-tests.json";

    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, get_conn_opt())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS odbc_table;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            match get_db_type() {
                DbType::MariaDb => {
                    r#"
CREATE TABLE odbc_table
(
    id int auto_increment primary key,
    name varchar(255) null,
    datetime datetime null
);"#
                }
                DbType::Postgres => {
                    r#"
CREATE TABLE odbc_table
(
    id SERIAL PRIMARY KEY,
    name VARCHAR(255),
    "datetime" TIMESTAMP NULL
);"#
                }
            },
            (),
            Some(3),
        )
        .unwrap();
    let _ = conn
        .execute(
            r#"
INSERT INTO odbc_table (name, datetime) VALUES
('test1', now()),
('test2', now()),
('test3', now()),
('test4', now()),
('test5', now());
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let params = ObjectMap::from([("id".into(), 0.into())]);

    fs::write(CONNECTION_STRING_FILE_PATH, conn_str).unwrap();
    fs::write(
        STATEMENT_FILE_PATH,
        "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;",
    )
    .unwrap();
    let _ = fs::remove_file(LAST_RUN_METADATA_PATH);

    let events = run_and_assert_source_compliance(
        OdbcConfig {
            connection_string_filepath: Some(CONNECTION_STRING_FILE_PATH.to_string()),
            schedule: Some("*/1 * * * * *".into()),
            statement_filepath: Some(STATEMENT_FILE_PATH.to_string()),
            statement_init_params: Some(params),
            tracking_columns: Some(vec!["id".to_string()]),
            last_run_metadata_path: Some(LAST_RUN_METADATA_PATH.to_string()),
            iterations: Some(5),
            ..Default::default()
        },
        Duration::from_secs(10),
        &SOURCE_TAGS,
    )
    .await;

    println!("{}", serde_json::to_string_pretty(&events).unwrap());
    assert_eq!(
        get_value_from_event(&events[0], "name"),
        Some("test1".into())
    );
    assert_eq!(
        get_value_from_event(&events[1], "name"),
        Some("test2".into())
    );
    assert_eq!(
        get_value_from_event(&events[2], "name"),
        Some("test3".into())
    );
    assert_eq!(
        get_value_from_event(&events[3], "name"),
        Some("test4".into())
    );
    assert_eq!(
        get_value_from_event(&events[4], "name"),
        Some("test5".into())
    );
}

#[tokio::test]
async fn query_number_types() {
    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, get_conn_opt())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS number_columns;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            match get_db_type() {
                DbType::MariaDb => {
                    r#"
create table number_columns
(
    int_col                int(10)                           null,
    bit_col                bit                               null,
    mediumint_col          mediumint                         null,
    middleint_col          mediumint                         null,
    smallint_col           smallint                          null,
    tinyint_col            tinyint                           null,
    bigint_col             bigint                            null,
    boolean_col            tinyint(1)                        null,
    double_col             double                            null,
    float_col              float                             null,
    decimal_col            decimal(10, 2)                    null
);
                "#
                }
                DbType::Postgres => {
                    r#"
CREATE TABLE number_columns
(
    int_col        INTEGER,            -- integer
    bit_col        BIT,                -- single bit (use BIT(n) to specify multiple bits)
    mediumint_col  INTEGER,            -- no MEDIUMINT in PostgreSQL, mapped to INTEGER
    middleint_col  INTEGER,            -- same as MEDIUMINT, mapped to INTEGER
    smallint_col   SMALLINT,           -- small integer
    tinyint_col    SMALLINT,           -- no TINYINT in PostgreSQL, mapped to SMALLINT
    bigint_col     BIGINT,             -- big integer (64-bit)
    boolean_col    BOOLEAN,            -- MySQL tinyint(1) mapped to BOOLEAN
    double_col     DOUBLE PRECISION,   -- MySQL DOUBLE mapped to PostgreSQL DOUBLE PRECISION
    float_col      REAL,               -- MySQL FLOAT mapped to PostgreSQL REAL (4-byte float)
    decimal_col    NUMERIC(10,2)       -- MySQL DECIMAL mapped to PostgreSQL NUMERIC(p,s)
);
                "#
                }
            },
            (),
            Some(3),
        )
        .unwrap();

    let _ = conn
        .execute(
            r#"
INSERT INTO number_columns (
    int_col,
    bit_col,
    mediumint_col,
    middleint_col,
    smallint_col,
    tinyint_col,
    bigint_col,
    boolean_col,
    double_col,
    float_col,
    decimal_col
) VALUES (
    -2147483648,
    b'0',
    -8388608,
    -8388608,
    -32768,
    -128,
    -9223372036854775808,
    FALSE,
    -1.7976931348623157e308,
    -3.402823466e38,
    -99999999.99
);
            "#,
            (),
            Some(3),
        )
        .unwrap();

    let _ = conn
        .execute(
            r#"
INSERT INTO number_columns (
    int_col,
    bit_col,
    mediumint_col,
    middleint_col,
    smallint_col,
    tinyint_col,
    bigint_col,
    boolean_col,
    double_col,
    float_col,
    decimal_col
) VALUES (
    2147483647,
    b'1',
    8388607,
    8388607,
    32767,
    127,
    9223372036854775807,
    TRUE,
    1.7976931348623157e308,
    3.402823466e38,
    99999999.99
);
            "#,
            (),
            Some(3),
        )
        .unwrap();

    let rows = execute_query(
        &env,
        &conn_str,
        "SELECT * FROM number_columns;",
        vec![],
        Duration::from_secs(3),
        Tz::UTC,
        10,
        1000,
    )
    .unwrap();
    println!("Rows Count: {}", rows.len());
    for row in &rows {
        if let Value::Object(map) = row {
            for (key, value) in map {
                println!("{}: {:?}", key, value);
            }
        }
    }

    let Value::Object(row) = &rows[0] else {
        panic!("No rows returned")
    };
    assert_eq!(*row.get("int_col").unwrap(), Value::Integer(-2147483648));
    match get_db_type() {
        DbType::MariaDb => assert_eq!(*row.get("bit_col").unwrap(), Value::Boolean(false)),
        DbType::Postgres => assert_eq!(
            *row.get("bit_col").unwrap(),
            Value::Bytes(Bytes::from_static(b"0"))
        ),
    }
    assert_eq!(*row.get("mediumint_col").unwrap(), Value::Integer(-8388608));
    assert_eq!(*row.get("middleint_col").unwrap(), Value::Integer(-8388608));
    assert_eq!(*row.get("smallint_col").unwrap(), Value::Integer(-32768));
    assert_eq!(*row.get("tinyint_col").unwrap(), Value::Integer(-128));
    assert_eq!(
        *row.get("bigint_col").unwrap(),
        Value::Integer(-9223372036854775808)
    );
    match get_db_type() {
        DbType::MariaDb => assert_eq!(*row.get("boolean_col").unwrap(), Value::Boolean(false)),
        DbType::Postgres => assert_eq!(
            *row.get("boolean_col").unwrap(),
            Value::Bytes(Bytes::from_static(b"0"))
        ),
    }
    assert_eq!(
        *row.get("double_col").unwrap(),
        Value::Float(NotNan::new(-1.7976931348623157e308).unwrap())
    );
    match get_db_type() {
        DbType::MariaDb => assert_eq!(
            *row.get("float_col").unwrap(),
            Value::Float(NotNan::new(-3.40282e38).unwrap())
        ),
        DbType::Postgres => assert_eq!(
            *row.get("float_col").unwrap(),
            Value::Float(NotNan::new(-3.4028235e38).unwrap())
        ),
    }
    assert_eq!(
        *row.get("decimal_col").unwrap(),
        Value::Float(NotNan::new(-99999999.99).unwrap())
    );

    let Value::Object(row) = &rows[1] else {
        panic!("No second row returned")
    };
    assert_eq!(*row.get("int_col").unwrap(), Value::Integer(2147483647));
    match get_db_type() {
        DbType::MariaDb => assert_eq!(*row.get("bit_col").unwrap(), Value::Boolean(true)),
        DbType::Postgres => assert_eq!(
            *row.get("bit_col").unwrap(),
            Value::Bytes(Bytes::from_static(b"1"))
        ),
    }
    assert_eq!(*row.get("mediumint_col").unwrap(), Value::Integer(8388607));
    assert_eq!(*row.get("middleint_col").unwrap(), Value::Integer(8388607));
    assert_eq!(*row.get("smallint_col").unwrap(), Value::Integer(32767));
    assert_eq!(*row.get("tinyint_col").unwrap(), Value::Integer(127));
    assert_eq!(
        *row.get("bigint_col").unwrap(),
        Value::Integer(9223372036854775807)
    );
    match get_db_type() {
        DbType::MariaDb => assert_eq!(*row.get("boolean_col").unwrap(), Value::Boolean(true)),
        DbType::Postgres => assert_eq!(
            *row.get("boolean_col").unwrap(),
            Value::Bytes(Bytes::from_static(b"1"))
        ),
    }
    assert_eq!(
        *row.get("double_col").unwrap(),
        Value::Float(NotNan::new(1.7976931348623157e308).unwrap())
    );
    match get_db_type() {
        DbType::MariaDb => assert_eq!(
            *row.get("float_col").unwrap(),
            Value::Float(NotNan::new(3.40282e38).unwrap())
        ),
        DbType::Postgres => assert_eq!(
            *row.get("float_col").unwrap(),
            Value::Float(NotNan::new(3.4028235e38).unwrap())
        ),
    }
    assert_eq!(
        *row.get("decimal_col").unwrap(),
        Value::Float(NotNan::new(99999999.99).unwrap())
    );

    println!("{:#?}", rows);
}

#[tokio::test]
async fn query_string_types() {
    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, get_conn_opt())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS string_columns;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            match get_db_type() {
                DbType::MariaDb => {
                    r#"
CREATE TABLE string_columns (
    char10_col        CHAR(10)       NULL,
    nchar10_col       NCHAR(10)      NULL,
    nvarchar10_col    NVARCHAR(10)   NULL,
    text_col          TEXT           NULL,
    tinytext_col      TINYTEXT       NULL,
    mediumtext_col    MEDIUMTEXT     NULL,
    longtext_col      LONGTEXT       NULL
) DEFAULT CHARSET = utf8mb3 COLLATE = utf8mb3_general_ci;
            "#
                }
                DbType::Postgres => {
                    r#"
CREATE TABLE string_columns (
    char10_col       CHAR(10),       -- fixed-length character column (10)
    nchar10_col      CHAR(10),       -- PostgreSQL has no NCHAR; use CHAR with UTF-8 encoding
    nvarchar10_col   VARCHAR(10),    -- PostgreSQL has no NVARCHAR; use VARCHAR with UTF-8 encoding
    text_col         TEXT,           -- unlimited length text
    tinytext_col     TEXT,           -- PostgreSQL has no TINYTEXT; use TEXT
    mediumtext_col   TEXT,           -- PostgreSQL has no MEDIUMTEXT; use TEXT
    longtext_col     TEXT            -- PostgreSQL has no LONGTEXT; use TEXT
);
                "#
                }
            },
            (),
            Some(3),
        )
        .unwrap();

    let _ = conn
        .execute(
            r#"
INSERT INTO string_columns (
    char10_col,
    nchar10_col,
    nvarchar10_col,
    text_col,
    tinytext_col,
    mediumtext_col,
    longtext_col
) VALUES (
    '0123456789',
    '0123456789',
    '0123456789',
    'text',
    'tinytext',
    'mediumtext',
    'longtext'
);
            "#,
            (),
            Some(3),
        )
        .unwrap();

    let rows = execute_query(
        &env,
        &conn_str,
        "SELECT * FROM string_columns;",
        vec![],
        Duration::from_secs(3),
        Tz::UTC,
        10,
        1000,
    )
    .unwrap();

    let Value::Object(row) = &rows[0] else {
        panic!("No rows returned")
    };

    assert_eq!(
        *row.get("char10_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"0123456789"))
    );
    assert_eq!(
        *row.get("nchar10_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"0123456789"))
    );
    assert_eq!(
        *row.get("nvarchar10_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"0123456789"))
    );
    assert_eq!(
        *row.get("text_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"text"))
    );
    assert_eq!(
        *row.get("tinytext_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"tinytext"))
    );
    assert_eq!(
        *row.get("mediumtext_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"mediumtext"))
    );
    assert_eq!(
        *row.get("longtext_col").unwrap(),
        Value::Bytes(Bytes::from_static(b"longtext"))
    );
}

#[tokio::test]
async fn query_timestamp_columns() {
    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, ConnectionOptions::default())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS timestamp_columns;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            match get_db_type() {
                DbType::MariaDb => r#"
CREATE TABLE timestamp_columns (
    date_col               DATE                              NULL,
    datetime_col           DATETIME                          NULL,
    time_col               TIME                              NULL,
    timestamp_col          TIMESTAMP                         NULL,
    year_col               YEAR                              NULL
) DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_general_ci;
                "#,
                DbType::Postgres => r#"
CREATE TABLE timestamp_columns (
    date_col       DATE,                          -- MySQL DATE → PostgreSQL DATE
    datetime_col   TIMESTAMP,                     -- MySQL DATETIME → PostgreSQL TIMESTAMP
    time_col       TIME,                          -- Same in both
    timestamp_col  TIMESTAMP,                     -- Same type (use TIMESTAMPTZ if timezone is needed)
    year_col       SMALLINT                       -- MySQL YEAR → PostgreSQL SMALLINT
);
                "#,
            },
            (),
            Some(3),
        )
        .unwrap();

    let _ = conn
        .execute(
            r#"
INSERT INTO timestamp_columns (
    date_col,
    datetime_col,
    time_col,
    timestamp_col,
    year_col
)
VALUES (
    '2025-10-04',
    '2025-10-04 12:34:56',
    '15:30:00',
    '2025-10-04 12:34:56',
    2025
);
                "#,
            (),
            Some(3),
        )
        .unwrap();

    let rows = execute_query(
        &env,
        &conn_str,
        "SELECT * FROM timestamp_columns;",
        vec![],
        Duration::from_secs(3),
        Tz::UTC,
        10,
        1000,
    )
    .unwrap();

    println!("Rows Count: {}", rows.len());
    for row in &rows {
        if let Value::Object(map) = row {
            for (key, value) in map {
                println!("{}: {:?}", key, value);
            }
        }
    }

    let Value::Object(row) = &rows[0] else {
        panic!("No rows returned")
    };

    assert_eq!(
        *row.get("date_col").unwrap(),
        Value::Timestamp(chrono::Utc.with_ymd_and_hms(2025, 10, 4, 0, 0, 0).unwrap())
    );
    assert_eq!(
        *row.get("datetime_col").unwrap(),
        Value::Timestamp(
            chrono::Utc
                .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
                .unwrap()
        )
    );
    assert_eq!(
        *row.get("time_col").unwrap(),
        Value::Timestamp(chrono::Utc.with_ymd_and_hms(1970, 1, 1, 15, 30, 0).unwrap())
    );
    assert_eq!(
        *row.get("timestamp_col").unwrap(),
        Value::Timestamp(
            chrono::Utc
                .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
                .unwrap()
        )
    );
    assert_eq!(*row.get("year_col").unwrap(), Value::Integer(2025));
}
