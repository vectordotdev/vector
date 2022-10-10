use clickhouse_rs::types::{SqlType, DateTimeType};
use chrono_tz::Tz;
use nom::{
    bytes::complete::{tag, take_until1},
    combinator::{all_consuming, map_res},
    character::complete::{u8 as p_u8, u32 as p_u32, u64 as p_u64},
    sequence::{delimited, preceded, pair},
    branch::alt,
    IResult
};

macro_rules! parse_static {
    ($($lit: literal, $method: ident, $id: ident),*) => {
        $(
            fn $method(input: &str) -> IResult<&str, SqlType> {
                let (rest, _) = tag($lit)(input)?;
                Ok((rest, SqlType::$id))
            }
        )*
    }
}

parse_static!{
    "UInt8", parse_uint8, UInt8,
    "UInt16", parse_uint16, UInt16,
    "UInt32", parse_uint32, UInt32,
    "UInt64", parse_uint64, UInt64,
    "Int8", parse_int8, Int8,
    "Int16", parse_int16, Int16,
    "Int32", parse_int32, Int32,
    "Int64", parse_int64, Int64,
    "Float32", parse_float32, Float32,
    "Float64", parse_float64, Float64,
    "String", parse_string, String,
    "UUID", parse_uuid, Uuid,
    "Date", parse_date, Date,
    "IPv4", parse_ipv4, Ipv4,
    "IPv6", parse_ipv6, Ipv6
}

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
        parse_uint8,
        parse_uint16,
        parse_uint32,
        parse_uint64,
        parse_int8,
        parse_int16,
        parse_int32,
        parse_int64,
        parse_float32,
        parse_float64,
        parse_string,
        parse_uuid,
        parse_date,
        parse_ipv4,
        parse_ipv6,
    ))(s)
}


fn parse_array(s: &str) -> IResult<&str, SqlType> {
    let (rest, v) = preceded(
        tag("Array"), 
        delimited(tag("("), parse_sql_type, tag(")"))
    )(s)?;
    Ok((rest, SqlType::Array(v.into())))
}

fn parse_map(s: &str) -> IResult<&str, SqlType> {
    let (rest, (k, v)) = preceded(
        tag("Map"),
        delimited(
            tag("("), 
            pair(parse_sql_type, preceded(tag(","), parse_sql_type)), 
            tag(")"))
    )(s)?;
    Ok((rest, SqlType::Map(k.into(), v.into())))
}

fn parse_nullable(s: &str) -> IResult<&str, SqlType> {
    let (rest, v) = preceded(
        tag("Nullable"),
        delimited(tag("("), parse_nullable_inner, tag(")"))
    )(s)?;
    Ok((rest, SqlType::Nullable(v.into())))
}

fn parse_nullable_inner(s: &str) -> IResult<&str, SqlType> {
    alt((
        parse_datetime64,
        parse_static_type,
        parse_fixed_string,
        parse_decimal,
    ))(s)
}

fn parse_datetime64(s: &str) -> IResult<&str, SqlType> {
    let (rest, (precision, tz)) = preceded(
        tag("DateTime64"),
        delimited(
            tag("("), 
            pair(
                p_u32, 
                preceded(
                    tag(","), 
                    map_res(take_until1(")"), |s: &str| s.parse::<Tz>())
                )
            ), 
            tag(")")
        )
    )(s)?;
    Ok((rest, SqlType::DateTime(DateTimeType::DateTime64(precision, tz))))
}

fn parse_fixed_string(s: &str) -> IResult<&str, SqlType> {
    let (rest, n) = preceded(
        tag("FixedString"),
        delimited(
            tag("("), 
            p_u64, 
            tag(")")
        ),
    )(s)?;
    Ok((rest, SqlType::FixedString(n as usize)))
}

fn parse_decimal(s: &str) -> IResult<&str, SqlType> {
    let (rest, (p, s)) = preceded(
        tag("Decimal"),
        delimited(
            tag("("), 
            pair(
                p_u8, 
                preceded(
                    tag(","),
                    p_u8,
                ),
            ), 
            tag(")"))
    )(s)?;
    Ok((rest, SqlType::Decimal(p, s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Tz;
    use clickhouse_rs::types::{SqlType,DateTimeType};

    #[test]
    fn test_parse_datetime64() {
        let table = vec![
            ("DateTime64(3,Asia/Shanghai)", SqlType::DateTime(DateTimeType::DateTime64(3, Tz::Asia__Shanghai))),
        ];
        for (s, expect) in table {
            let (_, actual) = parse_datetime64(s).unwrap();
            assert_eq!(actual, expect);
        }
    }

    #[test]
    fn test_table() {
        let table = vec![
            ("Nullable(UInt16)", SqlType::Nullable(SqlType::UInt16.into())),
            ("Array(Nullable(String))", SqlType::Array(SqlType::Nullable(SqlType::String.into()).into())),
            ("Map(Float32,Date)", SqlType::Map(SqlType::Float32.into(), SqlType::Date.into())),
            ("Map(Int64,FixedString(6))", SqlType::Map(SqlType::Int64.into(), SqlType::FixedString(6).into())),
            ("Map(Float64,Nullable(UUID))", SqlType::Map(
                SqlType::Float64.into(), 
                SqlType::Nullable(SqlType::Uuid.into()).into(),
            )),
            ("Map(DateTime64(3,Asia/Shanghai),Nullable(Decimal(9,5)))", SqlType::Map(
                SqlType::DateTime(DateTimeType::DateTime64(3, Tz::Asia__Shanghai)).into(),
                SqlType::Nullable(SqlType::Decimal(9, 5).into()).into(),
            )),
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
        let table = vec![
            "Nullable(Array(UInt8))",
            "Nullable(Map(String,String))",
        ];
        for s in table {
            assert!(parse_field_type(s).is_err())
        }
    }
}