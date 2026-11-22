# jTalk

jTalk is a procedural English voice synthesizer written in Rust. It renders NPC
dialogue from text and robotic vocals from a MusicXML score with embedded
lyrics. Both use the same formant renderer and work offline.

## Windows desktop app

Download the [portable Windows app](downloads/jTalk-Windows-x64.zip) for
Windows 10 or 11 on x64. Extract the ZIP and double-click **jTalk.exe**. Type
a line, adjust the voice, then select **Render & Play**. Use **Save WAV…** to
keep the result. The app starts with the [reference phrase](examples/reference-phrase.txt)
and 73% breath in the Default preset.

For singing, drop a MusicXML or MXL file onto the Windows app or select
**Open score…**. Choose a vocal line if the score has multiple parts or verses
with lyrics, then select **Render & Play**. The score supplies pitch and timing. The Default preset starts vocals at 22% breath.
Select **Use dialogue** to return to the text you were editing.

To build it from source on Windows, run `cargo build --release --bin jtalk-desktop`.

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

jTalk guesses pronunciations for unfamiliar words and names from their
spelling using an embedded American English model. This runs offline
for dialogue and MusicXML lyrics. Unknown tokens that contain digits are
read character by character. For example, `B7` is read as “B seven.”

Use `--pronunciations FILE` or the Rust `Pronunciations` API to specify an
exact reading. Each line in the file contains a word and its phones.
Stress digits are optional. Overrides take precedence over the lexicon
and the model.

```text
jtalk JH EY1 T AO1 K
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

Use `plan_song` and `render_singing` to render vocals from a MusicXML file.
`write_wav` writes any rendered audio into a Rust `Write` destination.
`PHONE_SYMBOLS` and `PHONE_VARIANTS` list the supported phone labels.

The pronunciation dictionary and its license are documented in
[data/README.md](data/README.md).

For the synthesis design, tuning values, and measured demo comparison, see
[Breath and consonant noise](docs/consonant-noise.md).

## Browser interface

```sh
cargo run --release -- serve
```

Open `http://127.0.0.1:8787` in your browser, type a line, and select
**Render speech**. Adjust pitch, speed, formant scale, or breathiness, then
render again to hear the change. The player can download the resulting WAV.
Use `--pronunciations FILE` when starting the server to supply custom words.
The browser UI handles dialogue. Use the `sing` command for MusicXML vocals.
