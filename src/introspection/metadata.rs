//! Const encoding of the two PyO3 introspection chunk shapes we emit.
//!
//! This intentionally depends on PyO3's implementation-level fragment and escaping APIs,
//! and its doc-hidden expression serializers. Keep these dependencies here: expressions
//! are owned by PyO3, while only the enclosing class/function schema is encoded below.

use pyo3::impl_::introspection::{
    escape_json_string, escaped_json_string_len, SerializedIntrospectionFragment,
};
use pyo3::inspect::{serialize_for_introspection, serialized_len_for_introspection, PyStaticExpr};

#[derive(Clone, Copy)]
pub(super) enum Chunk {
    Protocol {
        name: &'static str,
        id: &'static str,
    },
    Method {
        parent: &'static str,
        name: &'static str,
        arguments: &'static [(&'static str, PyStaticExpr)],
        returns: PyStaticExpr,
    },
}

impl Chunk {
    pub(super) const fn len(self) -> usize {
        self.encode::<0>().position
    }

    pub(super) const fn serialize<const N: usize>(self) -> SerializedIntrospectionFragment<N> {
        assert!(N == self.len(), "introspection fragment length mismatch");
        assert!(N <= u32::MAX as usize, "introspection fragment exceeds u32");
        let writer = self.encode::<N>();
        assert!(writer.position == N);
        SerializedIntrospectionFragment {
            length: N as u32,
            fragment: writer.bytes,
        }
    }

    const fn encode<const N: usize>(self) -> Writer<N> {
        let mut writer = Writer {
            bytes: [0; N],
            position: 0,
        };
        match self {
            Self::Protocol { name, id } => {
                writer.raw(r#"{"type":"class","attach_to_root":true,"name":"#);
                writer.string(name);
                writer.raw(r#","id":"#);
                writer.string(id);
                writer.raw(r#","bases":["#);
                writer.expression(&pyo3::type_hint_identifier!("typing", "Protocol"));
                writer.raw("]}");
            }
            Self::Method {
                parent,
                name,
                arguments,
                returns,
            } => {
                writer.raw(r#"{"type":"function","name":"#);
                writer.string(name);
                writer.raw(r#","parent":"#);
                writer.string(parent);
                writer.raw(r#","arguments":{"posonlyargs":[{"name":"self"}"#);
                let mut i = 0;
                while i < arguments.len() {
                    writer.raw(r#",{"name":"#);
                    writer.string(arguments[i].0);
                    writer.raw(r#","annotation":"#);
                    writer.expression(&arguments[i].1);
                    writer.raw("}");
                    i += 1;
                }
                writer.raw(r#"]},"returns":"#);
                writer.expression(&returns);
                writer.raw("}");
            }
        }
        writer
    }
}

/// Zero capacity counts bytes; any nonzero capacity writes the same encoding.
struct Writer<const N: usize> {
    bytes: [u8; N],
    position: usize,
}

impl<const N: usize> Writer<N> {
    const fn raw(&mut self, text: &str) {
        if N != 0 {
            let mut i = 0;
            while i < text.len() {
                self.bytes[self.position + i] = text.as_bytes()[i];
                i += 1;
            }
        }
        self.position += text.len();
    }

    const fn string(&mut self, text: &str) {
        self.raw("\"");
        self.position += if N == 0 {
            escaped_json_string_len(text)
        } else {
            escape_json_string(text, self.bytes.split_at_mut(self.position).1)
        };
        self.raw("\"");
    }

    const fn expression(&mut self, expression: &PyStaticExpr) {
        self.position += if N == 0 {
            serialized_len_for_introspection(expression)
        } else {
            serialize_for_introspection(expression, self.bytes.split_at_mut(self.position).1)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::inspect::PyStaticConstant;
    use pyo3::type_hint_union;
    use serde_json::{json, Value};

    const NAME: &str = "Protocol\"\\\n\r\t\u{0000}\u{0008}\u{000c}é🦀";
    const ID: &str = "library:\"\\\u{001f}日本語";
    const INT: PyStaticExpr = pyo3::type_hint_identifier!("builtins", "int");
    const NULLABLE: PyStaticExpr = pyo3::type_hint_union!(
        INT,
        PyStaticExpr::Constant {
            value: PyStaticConstant::None
        }
    );
    const CLASS: Chunk = Chunk::Protocol { name: NAME, id: ID };
    const METHOD: Chunk = Chunk::Method {
        parent: ID,
        name: "seek\"\\é",
        arguments: &[("offset\"\\\n🦀", INT), ("whence", INT)],
        returns: NULLABLE,
    };
    const NO_ARGUMENTS: Chunk = Chunk::Method {
        parent: ID,
        name: "tell",
        arguments: &[],
        returns: INT,
    };
    const CLASS_FRAGMENT: SerializedIntrospectionFragment<{ CLASS.len() }> = CLASS.serialize();
    const METHOD_FRAGMENT: SerializedIntrospectionFragment<{ METHOD.len() }> = METHOD.serialize();
    const NO_ARGUMENTS_FRAGMENT: SerializedIntrospectionFragment<{ NO_ARGUMENTS.len() }> =
        NO_ARGUMENTS.serialize();

    fn parse<const N: usize>(fragment: &SerializedIntrospectionFragment<N>) -> Value {
        assert_eq!(fragment.length as usize, N);
        // The repr(C) fragment is a native u32 prefix immediately followed by JSON.
        assert_eq!(
            std::mem::offset_of!(SerializedIntrospectionFragment<N>, length),
            0
        );
        assert_eq!(
            std::mem::offset_of!(SerializedIntrospectionFragment<N>, fragment),
            4
        );
        serde_json::from_slice(&fragment.fragment).unwrap()
    }

    #[test]
    fn protocol_is_attached_and_names_are_escaped() {
        assert_eq!(
            parse(&CLASS_FRAGMENT),
            json!({
                "type": "class", "attach_to_root": true, "name": NAME, "id": ID,
                "bases": [{
                    "type": "attribute", "value": {"type": "name", "id": "typing"},
                    "attr": "Protocol"
                }]
            })
        );
    }

    #[test]
    fn method_has_exact_positional_arguments_and_nullable_return() {
        let int = json!({"type": "name", "id": "int"});
        assert_eq!(
            parse(&METHOD_FRAGMENT),
            json!({
                "type": "function", "name": "seek\"\\é", "parent": ID,
                "arguments": {"posonlyargs": [
                    {"name": "self"},
                    {"name": "offset\"\\\n🦀", "annotation": int},
                    {"name": "whence", "annotation": int}
                ]},
                "returns": {
                    "type": "binop", "left": int, "op": "bitor",
                    "right": {"type": "constant", "kind": "none"}
                }
            })
        );
        assert_eq!(
            parse(&NO_ARGUMENTS_FRAGMENT)["arguments"],
            json!({"posonlyargs": [{"name": "self"}]})
        );
    }

    #[test]
    #[should_panic(expected = "introspection fragment length mismatch")]
    fn rejects_incorrect_buffer_length() {
        CLASS.serialize::<1>();
    }
}
