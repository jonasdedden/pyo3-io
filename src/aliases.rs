//! Named capability combinations, available without introspection or generated files.

use crate::file_types::{file_types, has_capability};

macro_rules! define_aliases {
    ($($alias:ident, $bound:ident: $mode:ident [$($capability:ident),+];)*) => {
        $(
            #[doc = concat!("A [`crate::Py", stringify!($mode), "File`] requiring `", stringify!($($capability),*), "`.")]
            pub type $alias = crate::PyFile<
                crate::$mode,
                { has_capability!(read; $($capability),*) },
                { has_capability!(write; $($capability),*) },
                { has_capability!(seek; $($capability),*) },
                { has_capability!(fileno; $($capability),*) },
            >;
            #[doc = concat!("A [`", stringify!($alias), "`] with an attached Python token. See [`crate::BoundFile`].")]
            pub type $bound<'py> = crate::BoundFile<
                'py,
                crate::$mode,
                { has_capability!(read; $($capability),*) },
                { has_capability!(write; $($capability),*) },
                { has_capability!(seek; $($capability),*) },
                { has_capability!(fileno; $($capability),*) },
            >;
        )*
    };
}

file_types!(define_aliases);
