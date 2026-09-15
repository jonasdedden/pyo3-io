//! Complete catalog of owned file aliases.
//!
//! Start with [`PyBinaryRead`], [`PyBinaryWrite`], [`PyBinaryReadSeek`], [`PyTextRead`],
//! [`PyTextWrite`] or [`PyTextReadSeek`]. Add `Write`, `Seek` or `Fileno` when the consumer
//! needs those capabilities. Names follow the order `Read`, `Write`, `Seek`, `Fileno`.
//!
//! Each alias lists its available operations and links to their family reference.
//! Already-attached forms are in [`bound`]; most callers obtain them with
//! [`bind`](crate::PyIO::bind) or [`into_bound`](crate::PyIO::into_bound)
//! without naming a bound type. All aliases remain importable from the crate root.

use crate::io_types::{has_capability, io_types, payload_type};

// Explicit fragments keep links on the family reference: rustdoc's Type::method resolution
// otherwise follows a type alias to the shared PyIO/BoundPyIO page.
macro_rules! method_link {
    ($family:expr, $method:ident) => {
        concat!(
            "[`",
            stringify!($method),
            "`](crate::",
            $family,
            "#method.",
            stringify!($method),
            ")"
        )
    };
}

macro_rules! operation_doc {
    (Binary, $family:expr, $common:ident, read) => {
        concat!(
            "- **Reading:** implements [`Read`](std::io::Read): ",
            method_link!($family, read),
            ", ",
            method_link!($family, read_exact),
            ", ",
            method_link!($family, read_to_end),
            ", ",
            method_link!($family, read_to_string),
            ", and the other `Read` methods. Import `std::io::Read` to call them.\n"
        )
    };
    (Binary, $family:expr, $common:ident, write) => {
        concat!(
            "- **Writing:** implements [`Write`](std::io::Write): ",
            method_link!($family, write),
            ", ",
            method_link!($family, write_all),
            ", ",
            method_link!($family, flush),
            ". Import `std::io::Write` to call them.\n"
        )
    };
    (Binary, $family:expr, $common:ident, seek) => {
        concat!(
            "- **Seeking:** implements [`Seek`](std::io::Seek): ",
            method_link!($family, seek),
            ", ",
            method_link!($family, rewind),
            ", ",
            method_link!($family, stream_position),
            ". Import `std::io::Seek` to call them.\n"
        )
    };
    (Text, $family:expr, $common:ident, read) => {
        concat!(
            "- **Reading characters:** ",
            method_link!($family, read_chars),
            ", ",
            method_link!($family, read_to_string),
            ". This does not implement `std::io::Read`.\n"
        )
    };
    (Text, $family:expr, $common:ident, write) => {
        concat!(
            "- **Writing characters:** ",
            method_link!($family, write_str),
            ", ",
            method_link!($family, write_all_str),
            ", ",
            method_link!($family, flush),
            ". This does not implement `std::io::Write`.\n"
        )
    };
    (Text, $family:expr, $common:ident, seek) => {
        concat!(
            "- **Opaque-cookie seeking:** ",
            method_link!($family, tell),
            ", ",
            method_link!($family, seek_to),
            ", ",
            method_link!($family, rewind),
            ", ",
            method_link!($family, seek_to_end),
            ". This does not implement `std::io::Seek`.\n"
        )
    };
    ($payload:ident, $family:expr, $common:ident, fileno) => {
        concat!(
            "- **Descriptor number:** ",
            method_link!(stringify!($common), fileno),
            ".\n"
        )
    };
}

#[cfg(unix)]
macro_rules! unix_operation_doc {
    ($common:ident, fileno) => {
        concat!(
            "- **Owned Unix descriptor:** ",
            method_link!(stringify!($common), try_clone_fd),
            " returns an independent `OwnedFd`. The `AsRawFd` compatibility trait may panic; ",
            "there is deliberately no `AsFd` borrow of the Python object's descriptor.\n"
        )
    };
    ($common:ident, $other:ident) => {
        ""
    };
}

macro_rules! define_aliases {
    ($($alias:ident, $bound:ident, $protocol:ident: $payload:ident [$($capability:ident),+];)*) => {
        $(
            #[doc = concat!("An owned `", stringify!($payload), "` stream requiring `", stringify!($($capability),*), "`.")]
            #[doc = "\n# Available operations\n\n\
                - **Construction:** [`new`](crate::PyIO::new), [`py_new`](crate::PyIO::py_new).\n\
                - **Binding:** [`bind`](crate::PyIO::bind), [`into_bound`](crate::PyIO::into_bound).\n\
                - **Object access:** [`as_py_object`](crate::PyIO::as_py_object), \
                  [`into_py_object`](crate::PyIO::into_py_object).\n\
                - **Common traits:** [`Clone`], [`Debug`](std::fmt::Debug), \
                  [`FromPyObject`](pyo3::FromPyObject).\n"]
            $(
                #[doc = operation_doc!($payload, concat!("Py", stringify!($payload), "IO"), PyIO, $capability)]
                #[cfg_attr(unix, doc = unix_operation_doc!(PyIO, $capability))]
            )*
            #[doc = concat!("\n# API reference\n\n\
                Rustdoc does not copy implementations onto convenience aliases. See \
                [`Py", stringify!($payload), "IO`](crate::Py", stringify!($payload), "IO) for I/O methods \
                and [`PyIO`](crate::PyIO) for shared operations. Only the capabilities listed \
                above are available on this alias.\n\nThe attached counterpart is [`",
                stringify!($bound), "`](crate::aliases::bound::", stringify!($bound),
                "). See the [quickstart](crate#quickstart) for usage examples.")]
            pub type $alias = crate::PyIO<
                payload_type!($payload),
                { has_capability!(read; $($capability),*) },
                { has_capability!(write; $($capability),*) },
                { has_capability!(seek; $($capability),*) },
                { has_capability!(fileno; $($capability),*) },
            >;
        )*
    };
}

io_types!(define_aliases);

/// Complete catalog of aliases tied to an attached Python token.
///
/// Prefer constructing an [owned alias](super) and calling
/// [`bind`](crate::PyIO::bind) or [`into_bound`](crate::PyIO::into_bound).
/// These names are mainly useful when writing explicit lifetime-bearing signatures.
/// They are also importable from the crate root.
pub mod bound {
    use crate::io_types::{has_capability, io_types, payload_type};

    macro_rules! define_bound_aliases {
        ($($alias:ident, $bound:ident, $protocol:ident: $payload:ident [$($capability:ident),+];)*) => {
            $(
                #[doc = concat!("An attached `", stringify!($payload), "` stream requiring `", stringify!($($capability),*), "`.")]
                #[doc = concat!("\nObtained from [`", stringify!($alias), "`](crate::aliases::",
                    stringify!($alias), ") with [`bind`](crate::PyIO::bind) or \
                    [`into_bound`](crate::PyIO::into_bound); it cannot outlive its Python token.\n")]
                #[doc = "\n# Available operations\n\n\
                    - **Object access:** [`as_py_object`](crate::BoundPyIO::as_py_object).\n\
                    - **Release the token:** [`unbind`](crate::BoundPyIO::unbind) returns the owned form.\n\
                    - **Common traits:** [`Clone`], [`Debug`](std::fmt::Debug).\n"]
                $(
                    #[doc = operation_doc!($payload, concat!("BoundPy", stringify!($payload), "IO"), BoundPyIO, $capability)]
                    #[cfg_attr(unix, doc = unix_operation_doc!(BoundPyIO, $capability))]
                )*
                #[doc = concat!("\n# API reference\n\n\
                    Rustdoc does not copy implementations onto convenience aliases. See \
                    [`BoundPy", stringify!($payload), "IO`](crate::BoundPy", stringify!($payload), "IO) \
                    for I/O methods and [`BoundPyIO`](crate::BoundPyIO) for shared operations. \
                    Only the capabilities listed above are available on this alias.")]
                pub type $bound<'py> = crate::BoundPyIO<
                    'py,
                    payload_type!($payload),
                    { has_capability!(read; $($capability),*) },
                    { has_capability!(write; $($capability),*) },
                    { has_capability!(seek; $($capability),*) },
                    { has_capability!(fileno; $($capability),*) },
                >;
            )*
        };
    }

    io_types!(define_bound_aliases);
}
