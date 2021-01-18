use crate::api::schema::components::ComponentsSortFieldName;
use async_graphql::{Enum, InputObject, InputType};

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(InputObject)]
#[graphql(concrete(name = "ComponentsSortField", params(ComponentsSortFieldName)))]
pub struct SortField<T: InputType> {
    pub field: T,
    pub direction: Direction,
}

/// Defines a type as sortable when provided with a SortField<T>
pub trait SortableByField<T: InputType> {
    fn sort(&self, rhs: &Self, field: &T) -> std::cmp::Ordering;
}

/// Performs an in-place sort against a slice of Sortable<T>, with the provided SortField<T>s
pub fn by_fields<T: InputType>(f: &mut [impl SortableByField<T>], sort_fields: &[SortField<T>]) {
    use std::cmp::Ordering;

    f.sort_by(|a, b| {
        let mut cmp = Ordering::Equal;
        for sf in sort_fields {
            if cmp != Ordering::Equal {
                break;
            }
            cmp = a.sort(b, &sf.field);
            if sf.direction == Direction::Desc {
                cmp = cmp.reverse();
            }
        }
        cmp
    })
}
