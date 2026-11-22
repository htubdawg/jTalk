//! Procedural English speech and scored singing with a shared formant renderer.

mod english;
mod musicxml;
mod synth;

pub use english::{Pronunciations, plan_speech, pronounce};
pub use musicxml::{SongOptions, plan_song};
pub use synth::{Audio, Voice, render, write_wav};

use std::error::Error;
use std::io;

/// A pronunciation or score feature that jTalk could not interpret.
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn invalid(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    io::Error::new(io::ErrorKind::InvalidData, message.into()).into()
}

/// The renderer's 44-phone reference inventory, based on the traditional
/// 24-consonant and 20-vowel analysis of Southern British English.
pub const PHONE_SYMBOLS: &[&str] = &[
    "AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH", "UW", "B",
    "CH", "D", "DH", "F", "G", "HH", "JH", "K", "L", "M", "N", "NG", "P", "R", "S", "SH", "T",
    "TH", "V", "W", "Y", "Z", "ZH", "AX", "LOT", "IA", "EA", "UA",
];

/// Additional reduced or syllabic phones accepted by the renderer.
pub const PHONE_VARIANTS: &[&str] = &["IX", "EL", "EM", "EN"];

#[derive(Clone, Debug, PartialEq)]
pub struct Phone {
    pub symbol: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pitch {
    pub time: f64,
    pub hz: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Times are seconds. Unoccupied intervals are silent. Pitch targets are
/// linearly interpolated and apply to voiced phones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub phones: Vec<Phone>,
    pub pitch: Vec<Pitch>,
    pub cues: Vec<Cue>,
    pub duration: f64,
}

fn midi_hz(note: f64) -> f64 {
    440.0 * 2.0_f64.powf((note - 69.0) / 12.0)
}
