# jTalk

jTalk is a procedural English voice synthesizer written in Rust. It renders NPC
dialogue from text and robotic vocals from a MusicXML score with embedded
lyrics. Both use the same formant renderer and work offline.

## Try it

```sh
cargo run --release -- speak "Stay with me. Can you hear me?" -o dialogue.wav
cargo run --release -- sing examples/stay-with-me.musicxml --preset robot -o vocal.wav
```

Output is mono PCM WAV at 48 kHz by default. Use `--sample-rate` to select a
different rate or `-o -` to write WAV to standard output. Run
`cargo run -- --help` for voice controls. `--formant-scale` changes the vocal
tract independently of pitch, and `--breath`, `--jitter`, and `--vibrato`
control airflow and pitch variation. `--pitch` applies to dialogue.

## Prepare a vocal score

1. Enter a single vocal melody with lyrics attached to its notes in MuseScore
   or another notation editor. Use lyric hyphenation for words with more than
   one syllable.
2. Use a lyric extender where one syllable spans multiple pitches. A tie
   also continues the current syllable. Unlyricized notes without a tie or
   extender are silent.
3. Export a partwise `.musicxml` or compressed `.mxl` file and pass it to
   `jtalk sing`. If there is more than one lyric-bearing line, select it with
   `--part` (part ID or exact name), `--voice` or `--staff`. The default is
   verse 1 when present. Use `--verse` to render another.

The score sets the note lengths, rests, and tempo changes. `--bpm` supplies
the initial tempo for scores without an opening tempo marking (default 120).
The renderer follows the written measure order. For repeats, alternate
endings, and navigation marks such as D.S., export an unfolded score with the
notes in performance order. The selected voice is monophonic. MIDI import
and additional score voices can be added separately.

## Pronunciation

The default offline lexicon is North American English. The renderer models
the 44 phones of a traditional Southern British reference inventory using
CMU-style labels, with `AX` for schwa, `LOT` for /ɒ/, and `IA EA UA` for the
centering diphthongs. Reduced and syllabic variants `IX EL EM EN` are also
available. British pronunciations can be supplied explicitly.

Use `--pronunciations FILE` for names, alternate pronunciations, or words
missing from the lexicon. Write one word followed by its phones per line.
Stress digits are optional. Spell out numerals in dialogue or provide an
override for them.

```text
jtalk JH EY1 T AO1 K
mythfall M IH1 TH F AO2 L
```

## Embed in a Rust project

Use jTalk as a Cargo dependency. The Rust API returns floating-point PCM and
word or lyric-syllable timing cues in seconds. Plans are editable and can
also be constructed directly for a custom phoneme or pitch sequence.

```rust
use jtalk::{Pronunciations, Voice, plan_speech, render};

fn line() -> jtalk::Result<jtalk::Audio> {
    let voice = Voice::default();
    let plan = plan_speech("Can you hear me?", voice, 1.0, &Pronunciations::default())?;
    render(&plan, voice, 48_000)
}
```

Use `plan_song` to get the corresponding plan from a MusicXML file.
`write_wav` writes any rendered audio into a Rust `Write` destination.
`PHONE_SYMBOLS` and `PHONE_VARIANTS` list the supported phone labels.

The pronunciation dictionary and its license are documented in
[data/README.md](data/README.md).
