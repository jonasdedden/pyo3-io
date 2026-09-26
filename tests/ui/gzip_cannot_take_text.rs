//! The motivating case: a function generic over `io::Read` cannot be handed a text object.
use pyo3_io::PyTextRead;
use std::io::Read;

fn main() {}

fn decompress(_stream: impl Read) {}

fn oops(file: PyTextRead) {
    decompress(file);
}
