use std::sync::LazyLock;

use loqui_g2p::{Dialect, MAX_WORD_CHARS, NeuralG2p};

use crate::english::Sound;
use crate::{Result, invalid};

static MODEL: LazyLock<Result<NeuralG2p>> =
    LazyLock::new(|| Ok(NeuralG2p::load(Dialect::American)?));

pub(crate) fn guess(word: &str) -> Result<Option<Vec<Sound>>> {
    if word.chars().count() > MAX_WORD_CHARS || word.bytes().any(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let model = MODEL.as_ref().map_err(|error| invalid(error.to_string()))?;
    Ok(model
        .phonemize(word)
        .and_then(|phones| from_kokoro(&phones)))
}

fn from_kokoro(phones: &str) -> Option<Vec<Sound>> {
    let mut result = Vec::new();
    let mut chars = phones.chars().peekable();
    let mut stress = 0;
    while let Some(character) = chars.next() {
        let symbol = match character {
            'ˈ' | 'ˌ' => {
                stress = if character == 'ˈ' { 1 } else { 2 };
                continue;
            }
            'ə' | 'ɜ' if chars.peek() == Some(&'ɹ') => {
                chars.next();
                "ER"
            }
            'ɑ' => "AA",
            'æ' => "AE",
            'ʌ' => "AH",
            'ɔ' => "AO",
            'W' => "AW",
            'I' => "AY",
            'ɛ' => "EH",
            'ɜ' => "ER",
            'A' => "EY",
            'ɪ' => "IH",
            'i' => "IY",
            'O' => "OW",
            'Y' => "OY",
            'ʊ' => "UH",
            'u' => "UW",
            'ə' | 'ᵊ' => "AX",
            'ᵻ' => "IX",
            'b' => "B",
            'ʧ' => "CH",
            'd' | 'ɾ' => "D",
            'ð' => "DH",
            'f' => "F",
            'ɡ' | 'g' => "G",
            'h' => "HH",
            'ʤ' => "JH",
            'k' => "K",
            'l' => "L",
            'm' => "M",
            'n' => "N",
            'ŋ' => "NG",
            'p' => "P",
            'ɹ' => "R",
            's' => "S",
            'ʃ' => "SH",
            't' | 'ʔ' => "T",
            'θ' => "TH",
            'v' => "V",
            'w' => "W",
            'j' => "Y",
            'z' => "Z",
            'ʒ' => "ZH",
            _ => return None,
        };
        let mut sound = Sound { symbol, stress: 0 };
        if sound.is_nucleus() {
            sound.stress = std::mem::take(&mut stress);
        }
        result.push(sound);
    }
    let first_vowel = result.iter().position(|sound| sound.is_nucleus())?;
    if result.iter().all(|sound| sound.stress == 0) {
        result[first_vowel].stress = 1;
    }
    Some(result)
}
