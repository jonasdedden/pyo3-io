//! Type stub protocols for [`PyFile`](crate::PyFile).
//!
//! Every payload kind and capability set gets a `typing.Protocol` naming exactly the methods the
//! Rust side will call on the object, and `FromPyObject::INPUT_TYPE` points at the matching one.
//!
//! These are plain structural protocols, and they are exact: because the payload kind is fixed by
//! the Rust type there is no `isinstance` dispatch happening at runtime for them to disagree with.
//! A duck-typed object with the right methods is accepted by the type checker and by the runtime
//! alike.
//!
//! The protocols are emitted as PyO3 introspection chunks, i.e. `#[used] #[no_mangle]` statics
//! holding length-prefixed JSON, which is the format `#[pyclass]` emits too. They are declared
//! with `"attach_to_root": true` because a library crate cannot know the introspection id of the
//! root module of whichever extension links it; the stub generator adopts them into that module
//! and drops the ones no annotation refers to. See `build.rs` for the generation.

use pyo3::impl_::concat::combine_to_array;
use pyo3::impl_::introspection::SerializedIntrospectionFragment;
use pyo3::inspect::{PyClassNameStaticExpr, PyStaticExpr};
use pyo3::type_hint_identifier;

include!(concat!(env!("OUT_DIR"), "/protocols.rs"));
