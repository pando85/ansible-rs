#[cfg(feature = "docs")]
pub mod lookup;
#[cfg(not(feature = "docs"))]
mod lookup;

use crate::error;
use crate::error::{Error, ErrorKind, Result};

use std::result::Result as StdResult;
use std::sync::LazyLock;

use minijinja::{
    context, Environment, Error as MinijinjaError, ErrorKind as MinijinjaErrorKind,
    UndefinedBehavior, Value,
};
use serde_yaml::value::Value as YamlValue;

const OMIT_MESSAGE: &str = "Param is omitted";

fn omit() -> StdResult<String, MinijinjaError> {
    Err(MinijinjaError::new(
        MinijinjaErrorKind::InvalidOperation,
        OMIT_MESSAGE,
    ))
}

fn init_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.add_function("omit", omit);
    lookup::add_lookup_functions(&mut env);
    env
}

static MINIJINJA_ENV: LazyLock<Environment<'static>> = LazyLock::new(init_env);

#[inline(always)]
pub fn render(value: YamlValue, vars: &Value) -> Result<YamlValue> {
    match value.clone() {
        YamlValue::String(s) => Ok(serde_yaml::from_str(&render_string(&s, vars)?)?),
        YamlValue::Number(_) => Ok(value),
        YamlValue::Bool(_) => Ok(value),
        YamlValue::Sequence(v) => Ok(YamlValue::Sequence(
            v.iter()
                .map(|x| render(x.clone(), vars))
                .collect::<Result<Vec<_>>>()?,
        )),
        YamlValue::Mapping(x) => {
            let mut rendered_map = serde_yaml::Mapping::new();
            let mut current_vars = vars.clone();

            for (k, v) in x.iter() {
                let rendered_value = render(v.clone(), &current_vars)?;
                current_vars = context! {
                    ..current_vars,
                    ..Value::from_serialize(
                        json!({
                            // safe unwrap: k is always a String
                            k.as_str().unwrap(): rendered_value.clone()
                        })
                    )
                };
                rendered_map.insert(k.clone(), rendered_value);
            }

            Ok(YamlValue::Mapping(rendered_map))
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("{value:?} is not a valid render value"),
        )),
    }
}

#[inline(always)]
pub fn render_string(s: &str, vars: &Value) -> Result<String> {
    let mut env = MINIJINJA_ENV.clone();
    trace!("rendering {:?}", &s);
    env.add_template("t", s)?;
    let tmpl = env.get_template("t").map_err(map_minijinja_error)?;
    tmpl.render(vars).map_err(map_minijinja_error)
}

#[inline(always)]
pub fn is_render_string(s: &str, vars: &Value) -> Result<bool> {
    match render_string(
        &format!("{{% if {s} %}}true{{% else %}}false{{% endif %}}"),
        vars,
    )?
    .as_str()
    {
        "false" => Ok(false),
        _ => Ok(true),
    }
}

fn map_minijinja_error(e: MinijinjaError) -> Error {
    let f = |e: &MinijinjaError| -> Option<bool> { Some(e.detail()? == OMIT_MESSAGE) };
    match f(&e) {
        Some(true) => error::Error::new(error::ErrorKind::OmitParam, OMIT_MESSAGE),
        _ => error::Error::from(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render() {
        let r_yaml = render(YamlValue::from(1), &context! {}).unwrap();
        assert_eq!(r_yaml, YamlValue::from(1));

        let r_yaml = render(YamlValue::from("yea"), &context! {}).unwrap();
        assert_eq!(r_yaml, YamlValue::from("yea"));
    }

    #[test]
    fn test_render_string() {
        let r_yaml = render_string("{{ yea }}", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1");

        let r_yaml = render_string("{{ yea }} ", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1 ");

        let r_yaml = render_string(" {{ yea }}", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, " 1");

        let r_yaml = render_string("{{ yea }}\n", &context! {yea => 1}).unwrap();
        assert_eq!(r_yaml, "1\n");
    }

    #[test]
    fn test_is_render_string() {
        let r_true = is_render_string("true", &context! {}).unwrap();
        assert!(r_true);
        let r_false = is_render_string("false", &context! {}).unwrap();
        assert!(!r_false);
        let r_true = is_render_string("boo == 'test'", &context! {boo => "test"}).unwrap();
        assert!(r_true);
    }

    #[test]
    fn test_render_string_omit() {
        let string = "{{ package_filters | default(omit()) }}";
        let e = render_string(string, &context! {}).unwrap_err();
        assert_eq!(e.kind(), error::ErrorKind::OmitParam)
    }
}
