//! KiCad file formats: board files, footprint libraries, and project files.

pub mod edit;
pub mod library;
pub mod netlist;
pub mod normalize;
pub mod project;
pub mod writer;

pub use library::Libraries;
