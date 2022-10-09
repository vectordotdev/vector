use clickhouse_rs::types::{SqlType, DateTimeType};
use chrono_tz::Tz;
use nom::{
    Err as NE,
    error::{Error, ErrorKind},
    bytes::complete::tag,
    character::complete::{u32 as p_u32, u64 as p_u64},
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
    "Date", parse_date, Date
}

pub(super) fn parse_sql_type<'a>(s: &'a str) -> IResult<&'a str, SqlType> {
    alt((
        parse_nullable_inner,
        parse_nullable,
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
    ))(s)
}


fn parse_array(s: &str) -> IResult<&str, SqlType> {
    preceded(
        tag("Array"), 
        delimited(tag("("), parse_sql_type, tag(")"))
    )(s)
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
        parse_static_type,
        parse_fixed_string,
        parse_datetime64,
    ))(s)
}

fn parse_tz(s: &str) -> IResult<&str, Tz> {
    match s.parse::<Tz>() {
        Ok(v) => Ok(("", v)),
        Err(_) => Err(NE::Error(Error::new(s, ErrorKind::OneOf)))
    }
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
                    parse_tz
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