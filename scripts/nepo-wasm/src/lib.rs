//! NEPO (Open Roberta) block rendering for Blockst.
//!
//! Deliberately a second plugin rather than a second mode of the Scratch one.
//! The two dialects share nothing at the geometry level — different notches,
//! different corner radii, different idea of what a block is made of — and the
//! prototype has to leave the existing Scratch output untouched to be worth
//! anything. What they do share sits above this crate, in Typst: the font
//! measurement pass and the image wrapper.

pub mod matrix;
pub mod measure;
pub mod model;
mod protocol;
pub mod render;
pub mod svg;
pub mod theme;
