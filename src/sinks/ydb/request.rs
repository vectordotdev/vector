use std::collections::HashMap;

use snafu::{ResultExt, Snafu};
use tracing::debug;
use vector_lib::event::Event;
use ydb::{Query, TableClient, TableDescription, Value, YdbError, ydb_params};

use super::{
    config::{InsertStrategy, choose_insert_strategy},
    mapper::{EventMapper, MappingError},
};

#[derive(Debug, Snafu)]
pub enum YdbRequestError {
    #[snafu(display("Event mapping error: {source}"))]
    Mapping { source: MappingError },

    #[snafu(display("YDB error: {source}"))]
    Ydb { source: YdbError },
}

pub struct YdbRequestHandler {
    strategy: InsertStrategy,
    rows: Vec<Value>,
    table_path: String,
    // temporal field to store columns names and types for UPSERT query
    columns: Vec<(String, String)>,
}

impl YdbRequestHandler {
    pub fn prepare(
        events: Vec<Event>,
        schema: &TableDescription,
        table_path: String,
    ) -> Result<Self, YdbRequestError> {
        let mapper = EventMapper::new(schema);
        let rows: Result<Vec<Value>, MappingError> = events
            .into_iter()
            .map(|event| mapper.map_event(event))
            .collect();

        let rows = rows.context(MappingSnafu)?;
        let strategy = choose_insert_strategy(schema);

        let columns: Vec<(String, String)> = schema
            .columns
            .iter()
            .filter_map(|col| {
                col.type_value.as_ref().ok().map(|val| {
                    let debug_str = format!("{:?}", val);
                    let type_name = debug_str.split('(').next().unwrap_or("");
                    let yql_type = if type_name == "Bytes" {
                        "String"
                    } else {
                        type_name
                    };
                    (col.name.clone(), yql_type.to_string())
                })
            })
            .collect();

        Ok(Self {
            strategy,
            rows,
            table_path,
            columns,
        })
    }

    pub async fn execute(self, table_client: &TableClient) -> Result<(), YdbRequestError> {
        let YdbRequestHandler {
            strategy,
            rows,
            table_path,
            columns,
        } = self;

        match strategy {
            InsertStrategy::BulkUpsert => {
                debug!(
                    message = "Using bulk_upsert",
                    table = %table_path,
                );
                table_client
                    .retry_execute_bulk_upsert(table_path, rows)
                    .await
                    .context(YdbSnafu)?;
            }
            InsertStrategy::Upsert => {
                debug!(
                    message = "Using UPSERT in transaction",
                    table = %table_path,
                );
                execute_upsert_in_transaction(table_client, table_path, rows, columns)
                    .await
                    .context(YdbSnafu)?;
            }
        }

        Ok(())
    }
}

fn build_declare_section(row: &Value, columns: &[(String, String)]) -> Result<String, YdbError> {
    let Value::Struct(s) = row.clone() else {
        return Err(YdbError::Custom("Expected Struct value".to_string()));
    };

    let map: HashMap<String, Value> = s.into();
    let fields: Vec<String> = columns
        .iter()
        .filter(|(name, _)| map.contains_key(name))
        .map(|(name, yql_type)| format!("    {}: {}", name, yql_type))
        .collect();

    let fields_str = fields.join(",\n");

    Ok(format!(
        "DECLARE $values AS List<Struct<\n{}\n>>;\n\n",
        fields_str
    ))
}

async fn execute_upsert_in_transaction(
    table_client: &TableClient,
    table_path: String,
    rows: Vec<Value>,
    columns: Vec<(String, String)>,
) -> Result<(), YdbError> {
    if rows.is_empty() {
        return Ok(());
    }

    table_client
        .retry_transaction(|mut tx| {
            let table_path = table_path.clone();
            let rows = rows.clone();
            let columns = columns.clone();

            async move {
                let type_example = rows.first().unwrap();
                let declare = build_declare_section(type_example, &columns)?;
                let upsert = format!(
                    "UPSERT INTO `{}`\nSELECT * FROM AS_TABLE($values);",
                    table_path
                );
                let yql = format!("{}{}", declare, upsert);

                debug!(
                    message = "Executing UPSERT in transaction",
                    table = %table_path,
                    rows_count = rows.len(),
                );

                let values_list = Value::list_from(type_example.clone(), rows)?;
                let query = Query::new(yql).with_params(ydb_params!("$values" => values_list));

                tx.query(query).await?;
                tx.commit().await?;
                Ok(())
            }
        })
        .await
        .map_err(|e| match e {
            ydb::YdbOrCustomerError::YDB(ydb_err) => ydb_err,
            ydb::YdbOrCustomerError::Customer(custom_err) => {
                YdbError::Custom(format!("Transaction error: {}", custom_err))
            }
        })
}
