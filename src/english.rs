use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::sync::LazyLock;

use flate2::read::GzDecoder;

use crate::{
    Cue, PHONE_SYMBOLS, PHONE_VARIANTS, Phone, Pitch, Plan, Result, Voice, guessing, invalid,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sound {
    pub symbol: &'static str,
    pub stress: u8,
}

impl Sound {
    pub fn is_nucleus(self) -> bool {
        VOWELS.contains(&self.symbol)
    }
}

const VOWELS: &[&str] = &[
    "AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH", "UW", "AX",
    "LOT", "IA", "EA", "UA", "IX", "EL", "EM", "EN",
];

fn sound(token: &str) -> Result<Sound> {
    let stress = token
        .as_bytes()
        .last()
        .filter(|digit| digit.is_ascii_digit());
    let base = stress.map_or(token, |_| &token[..token.len() - 1]);
    let strength = stress.map_or(1, |digit| digit - b'0');
    let reduced = match (base, stress) {
        ("AH", Some(b'0')) => "AX",
        ("IH", Some(b'0')) => "IX",
        _ => base,
    };
    let symbol = PHONE_SYMBOLS
        .iter()
        .chain(PHONE_VARIANTS)
        .copied()
        .find(|&known| known == reduced)
        .ok_or_else(|| invalid(format!("unknown phone {token}")))?;
    if strength > 2 || (stress.is_some() && !VOWELS.contains(&symbol)) {
        return Err(invalid(format!("invalid stress on {token}")));
    }
    Ok(Sound {
        symbol,
        stress: strength,
    })
}

static DICTIONARY: LazyLock<HashMap<String, Vec<Vec<Sound>>>> = LazyLock::new(|| {
    let mut text = String::new();
    GzDecoder::new(&include_bytes!("../data/cmudict.dict.gz")[..])
        .read_to_string(&mut text)
        .expect("bundled CMU dictionary must decode");
    let mut dictionary = HashMap::<String, Vec<Vec<Sound>>>::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(word) = fields.next() else { continue };
        if word.starts_with('#') {
            continue;
        }
        let phones: Vec<_> = fields
            .take_while(|field| !field.starts_with('#'))
            .map(|field| sound(field).expect("bundled CMU phone must be supported"))
            .collect();
        dictionary
            .entry(word.split('(').next().unwrap().to_owned())
            .or_default()
            .push(phones);
    }
    dictionary
});

/// Project-specific names, dialect choices, and intentional pronunciations.
#[derive(Clone, Debug, Default)]
pub struct Pronunciations(HashMap<String, Vec<Sound>>);

impl Pronunciations {
    pub fn insert(&mut self, word: &str, phones: &str) -> Result<()> {
        let key = normalize(word);
        if !key.chars().any(is_word_base) || key.chars().any(|c| !is_word(c)) {
            return Err(invalid("pronunciation keys must be single words"));
        }
        let phones: Vec<_> = phones
            .split_whitespace()
            .map(sound)
            .collect::<Result<_>>()?;
        if phones.is_empty() {
            return Err(invalid("a pronunciation must contain at least one phone"));
        }
        self.0.insert(key, phones);
        Ok(())
    }

    fn lookup(&self, word: &str) -> Option<&[Vec<Sound>]> {
        let key = normalize(word);
        let unquoted = key.trim_matches('\'');
        if let Some(phones) = self.0.get(&key).or_else(|| self.0.get(unquoted)) {
            return Some(std::slice::from_ref(phones));
        }
        DICTIONARY
            .get(&key)
            .or_else(|| DICTIONARY.get(unquoted))
            .map(Vec::as_slice)
    }

    fn resolve(&self, word: &str) -> Result<Cow<'_, [Sound]>> {
        if let Some(pronunciations) = self.lookup(word) {
            return Ok(Cow::Borrowed(&pronunciations[0]));
        }
        Ok(Cow::Owned(guess_word(word)?))
    }
}

fn normalize(text: &str) -> String {
    text.replace(['’', '‘'], "'").to_lowercase()
}

fn guess_word(word: &str) -> Result<Vec<Sound>> {
    let spelling = word.replace(['’', '‘'], "'");
    let spelling = spelling.trim_matches('\'');
    let valid = spelling.chars().all(is_word) && spelling.chars().any(is_word_base);
    if valid {
        let vowelless = spelling.is_ascii()
            && !spelling
                .to_ascii_lowercase()
                .contains(['a', 'e', 'i', 'o', 'u', 'y']);
        if !vowelless && let Some(phones) = guessing::guess(spelling)? {
            return Ok(phones);
        }
        if spelling.is_ascii() {
            return spell_out(spelling);
        }
    }
    Err(invalid(format!(
        "could not guess a pronunciation for {word:?}; add an override"
    )))
}

fn spell_out(spelling: &str) -> Result<Vec<Sound>> {
    const DIGITS: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let mut phones = Vec::new();
    for ch in spelling.bytes().filter(u8::is_ascii_alphanumeric) {
        let letter = [ch.to_ascii_lowercase()];
        let key = if ch.is_ascii_digit() {
            DIGITS[usize::from(ch - b'0')]
        } else {
            std::str::from_utf8(&letter).expect("ASCII letter")
        };
        if key == "a" {
            phones.push(sound("EY1")?);
        } else {
            let variants = DICTIONARY
                .get(key)
                .ok_or_else(|| invalid(format!("missing letter or digit name {key:?}")))?;
            phones.extend_from_slice(&variants[0]);
        }
    }
    Ok(phones)
}

fn is_word_base(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit()
}

fn is_word(c: char) -> bool {
    is_word_base(c) || matches!(c, '\'' | '’' | '‘')
}

pub(crate) fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c| !is_word(c))
        .filter(|word| word.chars().any(is_word_base))
}

/// Resolve a word to phones using an override, the CMU lexicon, or an offline guess.
pub fn pronounce(word: &str, pronunciations: &Pronunciations) -> Result<Vec<&'static str>> {
    Ok(pronunciations
        .resolve(word)?
        .iter()
        .map(|phone| phone.symbol)
        .collect())
}

pub(crate) fn lyric_syllables(
    word: &str,
    spelling: &[&str],
    pronunciations: &Pronunciations,
) -> Result<Vec<Vec<Sound>>> {
    if spelling.len() == 1 {
        let mut phrase = Vec::new();
        for token in words(word) {
            phrase.extend_from_slice(&pronunciations.resolve(token)?);
        }
        return Ok(vec![phrase]);
    }
    let matches_score = |sounds: &[Sound]| {
        sounds.iter().filter(|sound| sound.is_nucleus()).count() == spelling.len()
    };
    let syllable_error = || {
        invalid(format!(
            "no pronunciation for {word:?} with {} score syllables; check lyric hyphenation or add an override",
            spelling.len()
        ))
    };
    let sounds = if let Some(variants) = pronunciations.lookup(word) {
        Cow::Borrowed(
            variants
                .iter()
                .find(|sounds| matches_score(sounds))
                .ok_or_else(syllable_error)?
                .as_slice(),
        )
    } else {
        let guess = guess_word(word)?;
        if !matches_score(&guess) {
            let syllables = spelling
                .iter()
                .map(|part| pronunciations.resolve(part).map(Cow::into_owned))
                .collect::<Result<Vec<_>>>()?;
            if syllables
                .iter()
                .all(|sounds| sounds.iter().filter(|sound| sound.is_nucleus()).count() == 1)
            {
                return Ok(syllables);
            }
            return Err(syllable_error());
        }
        Cow::Owned(guess)
    };

    let nuclei: Vec<_> = sounds
        .iter()
        .enumerate()
        .filter_map(|(i, sound)| sound.is_nucleus().then_some(i))
        .collect();
    let mut start = 0;
    let mut syllables = Vec::new();
    for (i, pair) in nuclei.windows(2).enumerate() {
        let between = &sounds[pair[0] + 1..pair[1]];
        let next = normalize(spelling[i + 1]);
        let end = pair[0] + 1 + onset_start(between, &next);
        syllables.push(sounds[start..end].to_vec());
        start = end;
    }
    syllables.push(sounds[start..].to_vec());
    Ok(syllables)
}

fn onset_start(between: &[Sound], next: &str) -> usize {
    if next.starts_with(['a', 'e', 'i', 'o', 'u']) {
        return between.len();
    }
    for (index, phone) in between.iter().enumerate() {
        let aliases: &[&str] = match phone.symbol {
            "CH" => &["ch", "tch"],
            "SH" => &["sh", "ti", "ci", "ch"],
            "TH" | "DH" => &["th"],
            "JH" => &["j", "g", "dg"],
            "ZH" => &["s", "g", "z"],
            "K" => &["k", "c", "q", "ck"],
            "F" => &["f", "ph"],
            "HH" => &["h", "wh"],
            "Y" => &["y", "u"],
            "S" => &["s", "c"],
            "Z" => &["z", "s"],
            "NG" => &[],
            _ => &[phone.symbol],
        };
        if aliases
            .iter()
            .any(|alias| next.starts_with(&alias.to_ascii_lowercase()))
        {
            return index;
        }
    }
    between.len().saturating_sub(1)
}

pub(crate) fn weight(phone: Sound) -> f64 {
    match phone.symbol {
        "AW" | "AY" | "EY" | "OW" | "OY" | "IA" | "EA" | "UA" => 0.145,
        "AX" | "IX" | "EL" | "EM" | "EN" => 0.075,
        _ if phone.is_nucleus() => {
            if phone.stress == 0 {
                0.086
            } else {
                0.125
            }
        }
        "P" | "B" | "T" | "D" | "K" | "G" => 0.057,
        "S" | "SH" | "Z" | "CH" | "JH" => 0.076,
        _ => 0.055,
    }
}

pub(crate) fn distribute(plan: &mut Plan, sounds: &[Sound], start: f64, end: f64) {
    let total: f64 = sounds.iter().copied().map(weight).sum();
    let mut cursor = start;
    for (i, &phone) in sounds.iter().enumerate() {
        let boundary = if i + 1 == sounds.len() {
            end
        } else {
            cursor + (end - start) * weight(phone) / total
        };
        plan.phones.push(Phone {
            symbol: phone.symbol.to_owned(),
            start: cursor,
            end: boundary,
        });
        cursor = boundary;
    }
}

fn add_phrase(
    plan: &mut Plan,
    phrase: &[(&str, Vec<Sound>)],
    punctuation: char,
    hz: f64,
    speed: f64,
) {
    let length = phrase.len();
    for (i, (word, sounds)) in phrase.iter().enumerate() {
        let start = plan.duration;
        let seconds = sounds.iter().copied().map(weight).sum::<f64>() / speed;
        let end = start + seconds;
        let position = i as f64 / length as f64;
        let emphasis = if word.len() > 1 && word.chars().all(|c| !c.is_ascii_lowercase()) {
            1.12
        } else {
            1.0
        };
        let pitch =
            hz * (1.12 - 0.20 * position) * emphasis * if punctuation == '!' { 1.10 } else { 1.0 };
        let final_pitch = pitch
            * if i + 1 == length && punctuation == '?' {
                1.32
            } else {
                0.93
            };
        plan.pitch.push(Pitch {
            time: start,
            hz: pitch,
        });
        plan.pitch.push(Pitch {
            time: start + seconds * 0.86,
            hz: final_pitch,
        });
        distribute(plan, sounds, start, end);
        plan.cues.push(Cue {
            text: (*word).to_owned(),
            start,
            end,
        });
        plan.duration = end + 0.01 / speed;
    }
    plan.duration += match punctuation {
        '.' | '?' | '!' => 0.20,
        ',' | ';' | ':' | '—' => 0.11,
        _ => 0.0,
    } / speed;
}

/// Plan North American English dialogue, with punctuation-driven pauses and pitch.
pub fn plan_speech(
    text: &str,
    voice: Voice,
    speed: f64,
    pronunciations: &Pronunciations,
) -> Result<Plan> {
    if !speed.is_finite() || !(0.25..=4.0).contains(&speed) {
        return Err(invalid("speech speed must be between 0.25 and 4"));
    }
    let mut plan = Plan::default();
    let mut start = 0;
    for (end, punctuation) in text.char_indices().chain([(text.len(), '\0')]) {
        if !matches!(punctuation, '.' | '?' | '!' | ',' | ';' | ':' | '—' | '\0') {
            continue;
        }
        let phrase = words(&text[start..end])
            .map(|word| Ok((word, pronunciations.resolve(word)?.into_owned())))
            .collect::<Result<Vec<_>>>()?;
        if !phrase.is_empty() {
            add_phrase(&mut plan, &phrase, punctuation, voice.speech_hz, speed);
        }
        start = end + punctuation.len_utf8();
    }
    if plan.cues.is_empty() {
        return Err(invalid("speech text contains no English words"));
    }
    Ok(plan)
}
