use chrono_tz::Tz;
use clickhouse_rs::types::{DateTimeType, SqlType};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until1},
    character::complete::{u32 as p_u32, u64 as p_u64, u8 as p_u8},
    combinator::{all_consuming, map_res},
    sequence::{delimited, pair, preceded},
    IResult,
};

pub(super) fn parse_field_type(s: &str) -> IResult<&str, SqlType> {
    all_consuming(parse_sql_type)(s)
}

fn parse_sql_type(s: &str) -> IResult<&str, SqlType> {
    alt((
        // types that can be wrapped by Nullable(xxx)
        parse_nullable_inner,
        // Nullable
        parse_nullable,
        // types that can NOT be wrapped by Nullable
        parse_array,
        parse_map,
    ))(s)
}

fn parse_static_type(s: &str) -> IResult<&str, SqlType> {
    alt((
        |i| tag("UInt8")(i).map(|(rest, _)| (rest, SqlType::UInt8)),
        |i| tag("UInt16")(i).map(|(rest, _)| (rest, SqlType::UInt16)),
        |i| tag("UInt32")(i).map(|(rest, _)| (rest, SqlType::UInt32)),
        |i| tag("UInt64")(i).map(|(rest, _)| (rest, SqlType::UInt64)),
        |i| tag("Int8")(i).map(|(rest, _)| (rest, SqlType::Int8)),
        |i| tag("Int16")(i).map(|(rest, _)| (rest, SqlType::Int16)),
        |i| tag("Int32")(i).map(|(rest, _)| (rest, SqlType::Int32)),
        |i| tag("Int64")(i).map(|(rest, _)| (rest, SqlType::Int64)),
        |i| tag("Float32")(i).map(|(rest, _)| (rest, SqlType::Float32)),
        |i| tag("Float64")(i).map(|(rest, _)| (rest, SqlType::Float64)),
        |i| tag("String")(i).map(|(rest, _)| (rest, SqlType::String)),
        |i| tag("UUID")(i).map(|(rest, _)| (rest, SqlType::Uuid)),
        |i| tag("Date")(i).map(|(rest, _)| (rest, SqlType::Date)),
        |i| tag("IPv4")(i).map(|(rest, _)| (rest, SqlType::Ipv4)),
        |i| tag("IPv6")(i).map(|(rest, _)| (rest, SqlType::Ipv6)),
    ))(s)
}

fn parse_array(s: &str) -> IResult<&str, SqlType> {
    preceded(tag("Array"), delimited(tag("("), parse_sql_type, tag(")")))(s)
        .map(|(rest, v)| (rest, SqlType::Array(v.into())))
}

fn parse_map(s: &str) -> IResult<&str, SqlType> {
    preceded(
        tag("Map"),
        delimited(
            tag("("),
            pair(parse_sql_type, preceded(tag(","), parse_sql_type)),
            tag(")"),
        ),
    )(s)
    .map(|(rest, (k, v))| (rest, SqlType::Map(k.into(), v.into())))
}

fn parse_nullable(s: &str) -> IResult<&str, SqlType> {
    preceded(
        tag("Nullable"),
        delimited(tag("("), parse_nullable_inner, tag(")")),
    )(s)
    .map(|(rest, v)| (rest, SqlType::Nullable(v.into())))
}

fn parse_nullable_inner(s: &str) -> IResult<&str, SqlType> {
    alt((
        parse_datetime64,
        parse_datetime,
        parse_static_type,
        parse_fixed_string,
        parse_decimal,
    ))(s)
}

fn parse_datetime(s: &str) -> IResult<&str, SqlType> {
    tag("DateTime")(s).map(|(rest, _)| (rest, SqlType::DateTime(DateTimeType::DateTime32)))
}

fn parse_datetime64(s: &str) -> IResult<&str, SqlType> {
    preceded(
        tag("DateTime64"),
        delimited(
            tag("("),
            pair(
                p_u32,
                preceded(
                    tag(","),
                    map_res(take_until1(")"), |s: &str| s.parse::<Tz>()),
                ),
            ),
            tag(")"),
        ),
    )(s)
    .map(|(rest, (precision, tz))| {
        (
            rest,
            SqlType::DateTime(DateTimeType::DateTime64(precision, tz)),
        )
    })
}

fn parse_fixed_string(s: &str) -> IResult<&str, SqlType> {
    preceded(tag("FixedString"), delimited(tag("("), p_u64, tag(")")))(s)
        .map(|(rest, v)| (rest, SqlType::FixedString(v as usize)))
}

fn parse_decimal(s: &str) -> IResult<&str, SqlType> {
    preceded(
        tag("Decimal"),
        delimited(tag("("), pair(p_u8, preceded(tag(","), p_u8)), tag(")")),
    )(s)
    .map(|(rest, (p, s))| (rest, SqlType::Decimal(p, s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Tz;
    use clickhouse_rs::types::{DateTimeType, SqlType};

    #[test]
    fn test_parse_datetime64() {
        let table = vec![(
            "DateTime64(3,Asia/Shanghai)",
            SqlType::DateTime(DateTimeType::DateTime64(3, Tz::Asia__Shanghai)),
        )];
        for (s, expect) in table {
            let (_, actual) = parse_datetime64(s).unwrap();
            assert_eq!(actual, expect);
        }
    }

    #[test]
    fn test_table() {
        let table = vec![
            (
                "Nullable(UInt16)",
                SqlType::Nullable(SqlType::UInt16.into()),
            ),
            (
                "Array(Nullable(String))",
                SqlType::Array(SqlType::Nullable(SqlType::String.into()).into()),
            ),
            (
                "Map(Float32,Date)",
                SqlType::Map(SqlType::Float32.into(), SqlType::Date.into()),
            ),
            (
                "Map(Int64,FixedString(6))",
                SqlType::Map(SqlType::Int64.into(), SqlType::FixedString(6).into()),
            ),
            (
                "Map(Float64,Nullable(UUID))",
                SqlType::Map(
                    SqlType::Float64.into(),
                    SqlType::Nullable(SqlType::Uuid.into()).into(),
                ),
            ),
            (
                "Map(DateTime64(3,Asia/Shanghai),Nullable(Decimal(9,5)))",
                SqlType::Map(
                    SqlType::DateTime(DateTimeType::DateTime64(3, Tz::Asia__Shanghai)).into(),
                    SqlType::Nullable(SqlType::Decimal(9, 5).into()).into(),
                ),
            ),
            ("IPv4", SqlType::Ipv4),
            ("IPv6", SqlType::Ipv6),
        ];
        for (s, expect) in table {
            let (_, actual) = parse_field_type(s).unwrap();
            assert_eq!(actual, expect);
        }
    }

    #[test]
    fn test_nullable_cannot_wrap() {
        let table = vec!["Nullable(Array(UInt8))", "Nullable(Map(String,String))"];
        for s in table {
            assert!(parse_field_type(s).is_err())
        }
    }
}
