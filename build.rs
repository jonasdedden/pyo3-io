//! Generates the capability type aliases and the type stub protocols.
//!
//! There are two payload kinds and fifteen non-empty capability sets, so thirty of each. Every
//! protocol is a PyO3 introspection chunk needing its own statically named symbol, which is far
//! too many to spell out by hand.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// The capabilities, in the order they appear in `PyFile`'s parameters and in generated names.
const CAPABILITIES: [(&str, u8); 4] = [("Read", 8), ("Write", 4), ("Seek", 2), ("Fileno", 1)];

struct Combination {
    bits: u8,
    text: bool,
    /// e.g. `ReadFileno`
    suffix: String,
}

impl Combination {
    fn has(&self, flag: u8) -> bool {
        self.bits & flag != 0
    }

    /// e.g. `SupportsBinaryReadFileno`
    fn protocol(&self) -> String {
        format!(
            "Supports{}{}",
            if self.text { "Text" } else { "Binary" },
            self.suffix
        )
    }

    /// e.g. `BinaryReadFileno`
    fn alias(&self) -> String {
        format!(
            "{}{}",
            if self.text { "Text" } else { "Binary" },
            self.suffix
        )
    }

    /// e.g. `HINT_BINARY_1001`
    fn hint(&self) -> String {
        format!(
            "HINT_{}_{:04b}",
            if self.text { "TEXT" } else { "BINARY" },
            self.bits
        )
    }

    /// The payload type of `read` and `write` for this kind.
    fn payload(&self) -> &'static str {
        if self.text {
            "str"
        } else {
            "bytes"
        }
    }

    /// The methods the Rust side actually calls on the object.
    fn methods(&self) -> Vec<&'static str> {
        let mut methods = Vec::new();
        if self.has(8) {
            methods.push("read");
        }
        if self.has(4) {
            // `flush` is deliberately absent: a `Protocol` cannot mark a member optional, and the
            // implementation only calls it when the object has one, so requiring it here would
            // reject minimal writers the runtime is perfectly happy with.
            methods.push("write");
        }
        if self.has(2) {
            methods.push("seek");
            if self.text {
                // Text positions are opaque cookies, which only `tell` can produce.
                methods.push("tell");
            }
        }
        if self.has(1) {
            methods.push("fileno");
        }
        methods
    }
}

fn combinations() -> Vec<Combination> {
    let mut output = Vec::new();
    for &text in &[false, true] {
        for bits in 1..16u8 {
            let suffix = CAPABILITIES
                .iter()
                .filter(|(_, flag)| bits & flag != 0)
                .map(|(name, _)| *name)
                .collect::<String>();
            output.push(Combination { bits, text, suffix });
        }
    }
    output
}

/// A `builtins` type as an introspection expression.
fn builtin(name: &str) -> String {
    format!(r#"{{"type":"attribute","value":{{"type":"name","id":"builtins"}},"attr":"{name}"}}"#)
}

/// `_typeshed.ReadableBuffer`, everything the byte extraction accepts.
///
/// Wider than `bytes` because it is: `bytearray`, `memoryview` and `array` all work, and so does
/// the `buffer` attribute of a text stream, which typeshed types as returning a `ReadableBuffer`
/// and which the payload-kind error message points people at.
fn readable_buffer() -> String {
    r#"{"type":"attribute","value":{"type":"name","id":"_typeshed"},"attr":"ReadableBuffer"}"#
        .to_string()
}

/// The JSON of one protocol method, as a function chunk parented to the protocol.
///
/// Everything goes through `call_method0`/`call_method1`, so every argument is positional only.
fn method(method: &str, protocol: &str, payload: &str) -> String {
    let (arguments, returns) = match method {
        "read" => (
            format!(
                r#"[{{"name":"self"}},{{"name":"size","annotation":{}}}]"#,
                builtin("int")
            ),
            if payload == "str" {
                builtin("str")
            } else {
                readable_buffer()
            },
        ),
        // The result is extracted as the amount written, so unlike `_typeshed.SupportsWrite` this
        // really does need an `int` back
        "write" => (
            format!(
                r#"[{{"name":"self"}},{{"name":"data","annotation":{}}}]"#,
                builtin(payload)
            ),
            builtin("int"),
        ),
        // The result is discarded, like `_typeshed.SupportsFlush`
        "flush" => (r#"[{"name":"self"}]"#.to_string(), builtin("object")),
        "seek" => (
            format!(
                r#"[{{"name":"self"}},{{"name":"offset","annotation":{int}}},{{"name":"whence","annotation":{int}}}]"#,
                int = builtin("int")
            ),
            builtin("int"),
        ),
        "tell" | "fileno" => (r#"[{"name":"self"}]"#.to_string(), builtin("int")),
        other => panic!("unknown method {other}"),
    };
    format!(
        r#"{{"type":"function","name":"{method}","parent":"pyo3-file-typed:{protocol}","arguments":{{"posonlyargs":{arguments}}},"returns":{returns}}}"#
    )
}

/// A class chunk for a protocol.
///
/// `attach_to_root` asks the stub generator to adopt it into the root module of whichever
/// extension links this crate, which is something a library cannot name itself.
fn class(protocol: &str) -> String {
    format!(
        r#"{{"type":"class","attach_to_root":true,"name":"{protocol}","id":"pyo3-file-typed:{protocol}","bases":[{{"type":"attribute","value":{{"type":"name","id":"typing"}},"attr":"Protocol"}}]}}"#
    )
}

/// Emits a chunk as a statically linked, length-prefixed JSON blob.
///
/// `no_mangle` keeps the symbol name recognisable to the stub generator, `used` keeps the linker
/// from dropping a static nothing refers to.
fn emit_chunk(out: &mut String, symbol: &str, json: &str) {
    writeln!(
        out,
        r##"const _: () = {{
    const JSON: &str = r#"{json}"#;
    const LEN: usize = JSON.len();
    #[used]
    #[no_mangle]
    static {symbol}: SerializedIntrospectionFragment<LEN> = SerializedIntrospectionFragment {{
        length: LEN as u32,
        fragment: combine_to_array::<LEN>(&[JSON.as_bytes()]),
    }};
}};"##
    )
    .unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out_dir = Path::new(&env::var("OUT_DIR").unwrap()).to_path_buf();
    let combinations = combinations();

    let mut aliases = String::new();
    for combination in &combinations {
        let alias = combination.alias();
        let (mode, kind) = if combination.text {
            ("Text", "text")
        } else {
            ("Binary", "binary")
        };
        let capabilities = CAPABILITIES
            .iter()
            .map(|(name, flag)| {
                if combination.has(*flag) {
                    format!("`{}`", name.to_lowercase())
                } else {
                    String::new()
                }
            })
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let [read, write, seek, fileno] =
            CAPABILITIES.map(|(_, flag)| combination.has(flag).to_string());
        writeln!(
            aliases,
            "/// A {kind} object supporting {capabilities}.\n\
             pub type {alias} = PyFile<{mode}, {read}, {write}, {seek}, {fileno}>;"
        )
        .unwrap();
    }
    fs::write(out_dir.join("aliases.rs"), aliases).unwrap();

    let mut protocols = String::new();
    let mut arms = String::new();
    for combination in &combinations {
        let (hint, protocol) = (combination.hint(), combination.protocol());
        writeln!(
            protocols,
            r#"const {hint}: PyStaticExpr = PyStaticExpr::PyClass(PyClassNameStaticExpr::new(
    &PyStaticExpr::Name {{ id: "{protocol}" }},
    "pyo3-file-typed:{protocol}",
));"#
        )
        .unwrap();
        let symbol = format!("PYO3_INTROSPECTION_1_PYO3_FILE_TYPED_{hint}");
        emit_chunk(&mut protocols, &symbol, &class(&protocol));
        for name in combination.methods() {
            emit_chunk(
                &mut protocols,
                &format!("{symbol}_{}", name.to_uppercase()),
                &method(name, &protocol, combination.payload()),
            );
        }
        let [read, write, seek, fileno] =
            CAPABILITIES.map(|(_, flag)| combination.has(flag).to_string());
        writeln!(
            arms,
            "        ({}, {read}, {write}, {seek}, {fileno}) => {hint},",
            combination.text
        )
        .unwrap();
    }

    writeln!(
        protocols,
        r#"/// The protocol describing what a `PyFile` needs from the object it wraps.
///
/// Asking for no capabilities accepts anything, so it maps to `typing.Any`.
pub(crate) const fn protocol_hint(
    text: bool,
    read: bool,
    write: bool,
    seek: bool,
    fileno: bool,
) -> PyStaticExpr {{
    match (text, read, write, seek, fileno) {{
        (false, false, false, false, false) | (true, false, false, false, false) => {{
            type_hint_identifier!("typing", "Any")
        }}
{arms}    }}
}}"#
    )
    .unwrap();
    fs::write(out_dir.join("protocols.rs"), protocols).unwrap();
}
