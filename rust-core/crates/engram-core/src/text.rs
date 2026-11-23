use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Text(pub String);

impl Text {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Text {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0.contains('\n') {
            s.collect_seq(self.0.split('\n'))
        } else {
            s.serialize_str(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Text;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string or an array of lines")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Text, E> {
                Ok(Text(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Text, E> {
                Ok(Text(v))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Text, E> {
                Ok(Text::default())
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Text, A::Error> {
                let mut lines: Vec<String> = Vec::with_capacity(seq.size_hint().unwrap_or(8));
                while let Some(line) = seq.next_element::<String>()? {
                    lines.push(line);
                }
                Ok(Text(lines.join("\n")))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_is_string_multi_line_is_array() {
        assert_eq!(serde_json::to_string(&Text::from("one")).unwrap(), r#""one""#);
        assert_eq!(serde_json::to_string(&Text::from("a\nb\n")).unwrap(), r#"["a","b",""]"#);
    }

    #[test]
    fn roundtrip_both_shapes() {
        for s in ["", "x", "a\nb", "a\n\nb\n", "\n"] {
            let json = serde_json::to_string(&Text::from(s)).unwrap();
            assert_eq!(serde_json::from_str::<Text>(&json).unwrap().0, s);
        }
        assert_eq!(serde_json::from_str::<Text>(r#""a\nb""#).unwrap().0, "a\nb");
        assert_eq!(serde_json::from_str::<Text>("null").unwrap().0, "");
    }
}
