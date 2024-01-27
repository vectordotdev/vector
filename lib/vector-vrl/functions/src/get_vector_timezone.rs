use chrono::Local;
use std::borrow::Cow;
use vrl::prelude::*;

pub fn get_name_for_timezone(tz: &TimeZone) -> Cow<'_, str> {
    match tz {
        TimeZone::Named(tz) => tz.name().into(),
        TimeZone::Local => iana_time_zone::get_timezone()
            .unwrap_or_else(|_| Local::now().offset().to_string())
            .into(),
    }
}

fn get_vector_timezone(ctx: &mut Context) -> Resolved {
    Ok(get_name_for_timezone(ctx.timezone()).into())
}

#[derive(Clone, Copy, Debug)]
pub struct GetVectorTimezone;

impl Function for GetVectorTimezone {
    fn identifier(&self) -> &'static str {
        "get_vector_timezone"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Get the configured vector timezone name, or for 'local' the local timezone name or offset (e.g., -05:00)",
            source: r#"get_vector_timezone()"#,
            result: Ok("America/Chicago"),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(GetVectorTimezoneFn.as_expr())
    }
}

#[derive(Debug, Clone)]
struct GetVectorTimezoneFn;

impl FunctionExpression for GetVectorTimezoneFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        get_vector_timezone(ctx)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
