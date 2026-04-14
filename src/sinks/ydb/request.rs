use snafu::{ResultExt, Snafu};
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

        Ok(Self {
            strategy,
            rows,
            table_path,
        })
    }

    pub async fn execute(self, table_client: &TableClient) -> Result<(), YdbRequestError> {
        let YdbRequestHandler {
            strategy,
            rows,
            table_path,
        } = self;

        if rows.is_empty() {
            return Ok(());
        }

        match strategy {
            InsertStrategy::BulkUpsert => {
                table_client
                    .retry_execute_bulk_upsert(table_path, rows)
                    .await
                    .context(YdbSnafu)?;
            }
            InsertStrategy::Upsert => {
                execute_upsert_in_transaction(table_client, table_path, rows)
                    .await
                    .context(YdbSnafu)?;
            }
        }

        Ok(())
    }
}

async fn execute_upsert_in_transaction(
    table_client: &TableClient,
    table_path: String,
    rows: Vec<Value>,
) -> Result<(), YdbError> {
    if rows.is_empty() {
        return Ok(());
    }

    let type_example = rows
        .first()
        .expect("rows is not empty, checked above")
        .clone();

    let table_client = table_client
        .clone_with_transaction_options(ydb::TransactionOptions::new().with_autocommit(true));

    table_client
        .retry_transaction(|mut tx| {
            let table_path = table_path.clone();
            let rows = rows.clone();
            let type_example = type_example.clone();

            async move {
                let yql = format!(
                    "UPSERT INTO `{}`\nSELECT * FROM AS_TABLE($values);",
                    table_path
                );

                let values_list = Value::list_from(type_example, rows)?;
                let query = Query::new(yql).with_params(ydb_params!("$values" => values_list));

                tx.query(query).await?;
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
