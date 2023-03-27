use std::cmp::Ordering;

use async_graphql::{Enum, InputObject, InputType};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};

use crate::api::schema::{
    components::{
        sink::SinksSortFieldName, source::SourcesSortFieldName, transform::TransformsSortFieldName,
        ComponentsSortFieldName,
    },
    metrics::source::file::FileSourceMetricFilesSortFieldName,
};

#[derive(Enum, Copy, Clone, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Asc,
    Desc,
}

#[derive(InputObject)]
#[graphql(concrete(name = "ComponentsSortField", params(ComponentsSortFieldName)))]
#[graphql(concrete(name = "SourcesSortField", params(SourcesSortFieldName)))]
#[graphql(concrete(name = "TransformsSortField", params(TransformsSortFieldName)))]
#[graphql(concrete(name = "SinksSortField", params(SinksSortFieldName)))]
#[graphql(concrete(
    name = "FileSourceMetricFilesSortField",
    params(FileSourceMetricFilesSortFieldName)
))]
pub struct SortField<T: InputType> {
    pub field: T,
    #[graphql(default_with = "Direction::default()")]
    pub direction: Direction,
}

/// Defines a type as sortable by a given field
pub trait SortableByField<T: InputType> {
    fn sort(&self, rhs: &Self, field: &T) -> Ordering;
}

/// Performs an in-place sort against a slice of `Sortable<T>`, with the provided [`SortField<T>`]s
pub fn by_fields<T: InputType>(f: &mut [impl SortableByField<T>], sort_fields: &[SortField<T>]) {
    f.sort_by(|a, b| {
        sort_fields
            .iter()
            .fold_while(Ordering::Equal, |cmp, f| match cmp {
                Ordering::Equal => {
                    let cmp = a.sort(b, &f.field);
                    Continue(match f.direction {
                        Direction::Desc => cmp.reverse(),
                        _ => cmp,
                    })
                }
                _ => Done(cmp),
            })
            .into_inner()
    });
}
