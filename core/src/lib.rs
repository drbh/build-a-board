//! bb-core: the part of build-a-board that runs anywhere, the browser
//! included. It turns a board program's design into the resolved board model
//! and draws the preview, without Wasmtime, processes, or a filesystem.
//!
//! Inputs that live on disk natively (footprint libraries, artwork PNGs) come
//! in through [`footprint::FootprintSource`] and [`artwork::ArtworkSource`];
//! the design itself arrives as [`wit::Design`], a plain mirror of the WIT
//! types that `bb` fills from its Wasmtime bindings and a browser from JSON.

pub mod artwork;
pub mod design;
pub mod footprint;
pub mod preview;
pub mod resolve;
pub mod sexpr;
pub mod units;
pub mod wit;
