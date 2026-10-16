//! build-a-board: describe KiCad boards as WebAssembly components and build,
//! route, check, and release them.

pub use bb_core::{design, sexpr, units};

pub mod artwork;
pub mod board;
pub mod build;
pub mod commands;
pub mod generate;
pub mod json;
pub mod kicad;
pub mod polygon;
pub mod preview;
pub mod process;
pub mod release;
pub mod schematic;
pub mod specctra;
