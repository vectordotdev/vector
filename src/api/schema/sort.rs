use crate::api::schema::components::ComponentsSortFieldName;
use async_graphql::{Enum, InputObject, InputType};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use std::cmp::Ordering;

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Asc
    }
}

#[derive(InputObject)]
#[graphql(concrete(name = "ComponentsSortField", params(ComponentsSortFieldName)))]
pub struct SortField<T: InputType> {
    pub field: T,
    #[graphql(default_with = "Direction::default()")]
    pub direction: Direction,
}

/// Defines a type as sortable by a given field
pub trait SortableByField<T: InputType> {
    fn sort(&self, rhs: &Self, field: &T) -> Ordering;
}

/// Performs an in-place sort against a slice of Sortable<T>, with the provided SortField<T>s
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
