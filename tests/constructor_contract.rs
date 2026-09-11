//! Construction is a snapshot, not a cache of methods or a lifetime guarantee.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3_file_typed::{BinaryRead, BinaryWrite};
use std::io::{Read, Write};

#[test]
fn later_mutations_are_observed_and_validated() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::from_code(
            py,
            c"class Stream:\n def read(self, size): return b'a'\n def write(self, data): return len(data)\nstream = Stream()\n",
            c"constructor.py",
            c"constructor",
        ).unwrap();
        let object = module.getattr("stream").unwrap();
        let mut reader = BinaryRead::py_new(object.clone()).unwrap();
        let mut writer = BinaryWrite::py_new(object.clone()).unwrap();
        object.setattr("read", py.None()).unwrap();
        assert!(reader.read(&mut [0; 1]).is_err());
        object
            .setattr(
                "write",
                py.eval(c"lambda data: len(data) + 1", None, None).unwrap(),
            )
            .unwrap();
        assert_eq!(
            writer.write(b"a").unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    });
}
