//! Fully decoded JSON reference for classification fuzzing.
//!
//! Unlike serde_json::Value with the raw_value feature, object keys never
//! receive private-token semantics. Even ignored strings are fully decoded.
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Json {
    Other,
    String(String),
    Object { level: Option<String> },
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Json;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON value")
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Other)
            }
            fn visit_bool<E>(self, _: bool) -> Result<Json, E> {
                Ok(Json::Other)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Json, E> {
                Ok(Json::Other)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Json, E> {
                Ok(Json::Other)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Json, E> {
                Ok(Json::Other)
            }
            fn visit_str<E>(self, value: &str) -> Result<Json, E> {
                Ok(Json::String(value.into()))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Json, S::Error> {
                while seq.next_element::<Json>()?.is_some() {}
                Ok(Json::Other)
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Json, M::Error> {
                let mut level = None;
                while let Some((key, value)) = map.next_entry::<String, Json>()? {
                    if key == "level" {
                        level = match value {
                            Json::String(value) => Some(value),
                            _ => None,
                        };
                    }
                }
                Ok(Json::Object { level })
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_token_keys_are_ordinary_json_at_every_depth() {
        for input in [
            r#"{"$serde_json::private::RawValue":2,"level":"error"}"#,
            r#"{"nested":{"$serde_json::private::RawValue":2},"level":"error"}"#,
            r#"{"$serde_json::private::RawValue":"{\"level\":\"info\"}","level":"error"}"#,
            r#"{"$serde_json::private::Raw\u0056alue":2,"level":"error"}"#,
        ] {
            assert_eq!(
                serde_json::from_str::<Json>(input).unwrap(),
                Json::Object {
                    level: Some("error".into())
                },
                "{input}"
            );
        }
    }

    #[test]
    fn ignored_values_are_decoded_and_validated() {
        for input in [
            r#"{"nested":[{"message":"\uD800"}]}"#,
            r#"{"nested":{"$serde_json::private::RawValue":2,}}"#,
            r#"{"nested":1e400}"#,
        ] {
            assert!(serde_json::from_str::<Json>(input).is_err(), "{input}");
        }
        assert_eq!(
            serde_json::from_str::<Json>(
                r#"{"level":"e\u0072ror","nested":[null,true,1,-1,1.5,"\uD83D\uDE00"]}"#
            )
            .unwrap(),
            Json::Object {
                level: Some("error".into())
            }
        );
    }
}
