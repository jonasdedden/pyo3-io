//! A `typing.Protocol` per payload kind and capability set, naming the methods the Rust side
//! calls, for `FromPyObject::INPUT_TYPE` to point at.
//!
//! Each is emitted as `#[used]` statics in `#[pyclass]`'s introspection format. A library cannot
//! know the id of the extension module that links it, so they are marked `attach_to_root` and the
//! stub generator adopts the ones an annotation refers to.

use crate::io_types::{has_capability, io_types, payload_type};
use crate::Payload;
use pyo3::impl_::introspection::SerializedIntrospectionFragment;
use pyo3::inspect::{PyClassNameStaticExpr, PyStaticConstant, PyStaticExpr};
use pyo3::{type_hint_identifier, type_hint_union};

mod metadata;
use metadata::Chunk;

const INT: PyStaticExpr = type_hint_identifier!("builtins", "int");
const TEXT: PyStaticExpr = type_hint_identifier!("builtins", "str");
const BYTES: PyStaticExpr = type_hint_identifier!("builtins", "bytes");
const NONE: PyStaticExpr = PyStaticExpr::Constant {
    value: PyStaticConstant::None,
};
const BINARY_READ: PyStaticExpr =
    type_hint_union!(type_hint_identifier!("_typeshed", "ReadableBuffer"), NONE);
const TEXT_READ: PyStaticExpr = type_hint_union!(TEXT, NONE);
const WRITE_COUNT: PyStaticExpr = type_hint_union!(INT, NONE);
const ANY: PyStaticExpr = type_hint_identifier!("typing", "Any");

macro_rules! protocol_id {
    ($protocol:ident) => {
        concat!("pyo3-io:Supports", stringify!($protocol))
    };
}

// The protocol stem and suffix make the exported symbol unique.
macro_rules! emit_fragment {
    ($protocol:ident, $suffix:ident, $chunk:expr) => {
        const _: () = {
            const CHUNK: Chunk = $chunk;
            const LEN: usize = CHUNK.len();
            #[used]
            #[export_name = concat!("PYO3_INTROSPECTION_1_PYO3_IO_", stringify!($protocol), "_", stringify!($suffix))]
            static FRAGMENT: SerializedIntrospectionFragment<LEN> = CHUNK.serialize::<LEN>();
        };
    };
}

macro_rules! emit_method {
    ($protocol:ident, $name:ident, [$($argument:literal: $annotation:expr),*], $returns:expr) => {
        emit_fragment!($protocol, $name, Chunk::Method {
            parent: protocol_id!($protocol),
            name: stringify!($name),
            arguments: &[$(($argument, $annotation)),*],
            returns: $returns,
        });
    };
}

macro_rules! emit_text_tell {
    ($protocol:ident, Binary) => {};
    ($protocol:ident, Text) => {
        emit_method!($protocol, tell, [], INT);
    };
}

// Text seeking also needs tell. flush is optional at runtime, so no protocol requires it.
macro_rules! emit_capability {
    ($protocol:ident, $payload:ident, read) => {
        emit_method!($protocol, read, ["size": INT],
            if <payload_type!($payload)>::IS_TEXT { TEXT_READ } else { BINARY_READ });
    };
    ($protocol:ident, $payload:ident, write) => {
        emit_method!($protocol, write,
            ["data": if <payload_type!($payload)>::IS_TEXT { TEXT } else { BYTES }], WRITE_COUNT);
    };
    ($protocol:ident, $payload:ident, seek) => {
        emit_method!($protocol, seek, ["offset": INT, "whence": INT], INT);
        emit_text_tell!($protocol, $payload);
    };
    ($protocol:ident, $payload:ident, fileno) => {
        emit_method!($protocol, fileno, [], INT);
    };
}

const fn index(text: bool, read: bool, write: bool, seek: bool, fileno: bool) -> usize {
    ((text as usize) << 4)
        | ((read as usize) << 3)
        | ((write as usize) << 2)
        | ((seek as usize) << 1)
        | fileno as usize
}

macro_rules! define_protocols {
    ($($alias:ident, $bound:ident, $protocol:ident: $payload:ident [$($capability:ident),+];)*) => {
        $(
            emit_fragment!($protocol, protocol, Chunk::Protocol {
                name: concat!("Supports", stringify!($protocol)),
                id: protocol_id!($protocol),
            });
            $(emit_capability!($protocol, $payload, $capability);)*
        )*

        const HINTS: [PyStaticExpr; 32] = {
            let mut hints = [ANY; 32];
            $(
                hints[index(
                    <payload_type!($payload)>::IS_TEXT,
                    has_capability!(read; $($capability),*),
                    has_capability!(write; $($capability),*),
                    has_capability!(seek; $($capability),*),
                    has_capability!(fileno; $($capability),*),
                )] = PyStaticExpr::PyClass(PyClassNameStaticExpr::new(
                    &PyStaticExpr::Name { id: concat!("Supports", stringify!($protocol)) },
                    protocol_id!($protocol),
                ));
            )*
            hints
        };
    };
}

io_types!(define_protocols);

/// The protocol for a payload kind and capability set, or `typing.Any` if nothing is required.
pub(crate) const fn protocol_hint(
    text: bool,
    read: bool,
    write: bool,
    seek: bool,
    fileno: bool,
) -> PyStaticExpr {
    HINTS[index(text, read, write, seek, fileno)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selector_names_exactly_its_capabilities() {
        for text in [false, true] {
            for bits in 0..16 {
                let read = bits & 8 != 0;
                let write = bits & 4 != 0;
                let seek = bits & 2 != 0;
                let fileno = bits & 1 != 0;
                let hint = protocol_hint(text, read, write, seek, fileno);
                let expected = if bits == 0 {
                    "typing.Any".to_owned()
                } else {
                    format!(
                        "Supports{}{}{}{}{}",
                        if text { "Text" } else { "Binary" },
                        if read { "Read" } else { "" },
                        if write { "Write" } else { "" },
                        if seek { "Seek" } else { "" },
                        if fileno { "Fileno" } else { "" },
                    )
                };
                assert_eq!(hint.to_string(), expected);
            }
        }
    }
}
