# Voice synthesis design

jTalk renders voicing and consonant turbulence as separate signals. Dialogue and
MusicXML singing share this part of the renderer. The pronunciation planner
places `W IH DH` in “with” by default because the first entry in the bundled
dictionary uses the voiced dental fricative. The alternative `TH` is the
unvoiced sound in “thin.” Override the pronunciation to pick the alternate
without changing how either phone is synthesized.

## Signal paths

### Glottal source

The glottal flow has a smooth raised-cosine opening and a quarter-cosine
closing that ends abruptly. The renderer uses the flow's derivative, which
includes radiation at the lips and keeps the upper harmonics that the
closure excites. Speech and singing use the same source. A high shelf at
1 kHz adds presence, less at higher Breath settings because breathy voices
are darker. The open part of each cycle lengthens with Breath, from 55% of
the period for dialogue and 50% for singing.

`Voice::jitter_cents` sets the per-cycle pitch jitter. It also scales the
per-cycle amplitude shimmer (±4% at the default of 3 cents) and a slow,
mean-reverting pitch wander of about ±9 cents. The robot preset turns all
three off.

### Vocal tract

The source passes through formant resonators in series. Each resonator has
unity gain at 0 Hz, so a vowel's relative formant levels follow from its
formant frequencies and no per-vowel gain table is needed. F1 to F3 come
from the phone. Their bandwidths widen with frequency: `40 + 0.06·F1`,
`60 + 0.03·F2` and `100 + 0.03·F3` Hz. F4, F5 and F6 are fixed at 3.3, 3.75
and 4.9 kHz for dialogue. Singing moves F4 and F5 to 2.95 and 3.4 kHz,
next to F3. That cluster forms a singer's formant near 3 kHz.

Formants glide toward each phone's targets with a 12 ms time constant.
Consonant targets therefore bend the neighbouring vowels, as articulators
do. A nasal pole at 270 Hz and a matching zero are always in the chain. For
oral sounds the zero sits on the pole and cancels it. For nasals it moves
to 900 Hz (M), 1.6 kHz (N) or 3 kHz (NG), and an 18 ms glide nasalizes the
vowel edges next to them. Output fades over the attack and release at the
edges of a phrase, so the tract's ringing is never cut off.

### Voicing and breath

At a boundary between two voiced phones, both sides meet at the average of
their voicing gains. Smooth ramps link each phone's own gain to that
boundary value. Boundaries with silence or unvoiced phones fade to zero.

Breath starts with seeded white noise and passes through a one-pole
low-pass at 1.4 kHz. It is scaled by the glottal flow and `Voice::breath`,
which concentrates it in the open part of the cycle, and enters the vocal
tract with the voiced source. The CLI and library default to 0.035. The
browser and Windows app start at 73% for dialogue, and the Windows app
starts MusicXML vocals at 22%. Breath levels were calibrated so those
settings keep their earlier noise-to-voice balance.

### Aspiration

H is aspiration noise sent through the vocal tract, with the formants of
the following sound. After a voiceless stop (`P T K`), a voiced phone starts
with up to 50 ms of aspiration while its voicing fades in. Stops after S,
as in "stay", are not aspirated.

### Singing voice

MusicXML rendering uses `render_singing`. Beyond the singer's formant, it
gives vibrato a delayed onset. Depth fades in from 0.12 s to 0.42 s after
the pitch settles on a note, and each vibrato cycle varies its rate by up
to 6%. The planner starts onset consonants ahead of the beat, taking time
from a rest or up to half of the previous phone, so the vowel arrives on the
beat. Every change of note glides over 50 ms into the new pitch. The pitch
contour is also smoothed with a 15 ms time constant in both modes.

### Output level

`Voice::gain` sets the output peak. The render is normalized so its peak is
`1.12 × gain`, and samples above 0.75 are rounded off by a soft limiter
instead of clipping. The default gain of 0.8 peaks at about −1 dBFS.

### Consonant turbulence

Each noisy phone has two resonators at the same center frequency. The
second has twice the bandwidth of the first, so it lowers off-band energy
further and keeps most of the central band. A gain of 1.7 offsets part of
the loss from that second stage. Both states start fresh for each phone,
so energy from an S cannot
carry into the closure of the following T. The consonant gain has an
approximate square-root correction for sample rate, referenced to 48 kHz,
because fixed-width filters pass a smaller fraction of white noise as the
sample rate increases. The per-phone center frequency still follows the
voice's formant scale.

| Sound | Noise center | Bandwidth | Noise gain | Voicing gain |
| --- | ---: | ---: | ---: | ---: |
| S | 4,750 Hz | 1,100 Hz | 0.135 | 0 |
| T | 3,550 Hz | 1,200 Hz | 0.25 | 0 |
| TH | 2,650 Hz | 1,200 Hz | 0.115 | 0 |
| DH | 2,250 Hz | 1,100 Hz | 0.065 | 0.52 |

These are jTalk tuning values at a formant scale of 1.0. The higher center
frequency on S keeps it distinct from the dental fricatives. DH gets most
of its energy from voicing. TH has a little more noise to preserve the
unvoiced articulation. Other fricatives retain their own bands and levels.

Steady fricatives have a rounded attack and release. Oral stops
(`P B T D K G`) keep their noise at zero through closure and release it
during the final 12 ms, or the final 40% if the phone is shorter than 30 ms.
Voiced stops can carry quiet voicing during that closure. Voiced
fricatives pulse with the glottal cycle. Affricates
(`CH JH`) retain a longer friction tail. The robot preset turns off glottal
breath, so it still has the noise needed to articulate these consonants.

## Verification

The first breath and dental-fricative pass (`7b96cd3`) produced these
results against `ed4c698` at 48 kHz:

```sh
cargo run --release -- sing examples/stay-with-me.musicxml --preset robot -o vocal.wav
cargo run --release -- speak "Stay with me. Can you hear me?" -o dialogue.wav
```

| Window | Change in energy at or above 3 kHz |
| --- | ---: |
| Voiced DH in “with,” 2.350–2.390 s of the vocal | −21.3 dB |
| Full dialogue | −12.8 dB |

For each window, apply a Hann window to the PCM samples, take a real FFT,
and compare the sum of squared bin magnitudes at or above 3 kHz with
`10 × log10(new / previous)`. Dialogue RMS across the full band moved
from 0.0926 to 0.0924. These measurements describe the demo signals.
Intelligibility and timbre still depend on the line, pitch, and voice.

The second resonator stage was evaluated against `b624a8a` with both
dialogue renders at 35% breath:

```sh
cargo run --release -- speak "Stay with me. Can you hear me? The weather is clear. Take the north road. She sees six ships." --breath 0.35 -o dialogue.wav
```

| Window | Change above 6 kHz | Change above 8 kHz |
| --- | ---: | ---: |
| Full dialogue | −14.1 dB | −19.9 dB |
| Initial S, 0.012–0.060 s | −12.2 dB | −18.5 dB |

The vowel window at 0.160–0.220 s was sample-for-sample identical.
The same Hann-window and FFT comparison was used for both passes.

Compared with `3e92800`, the current mix reduces energy between
2.5 and 6 kHz by 5.2 dB in the full reference dialogue at 73% breath.
For the first sustained vowel in the example vocal, the fundamental
is 10.2 dB lower relative to its upper harmonics. That comparison
uses the same 73% breath setting for both vocal renders.

The vocal tract and source rewrite was measured against `6542f76` on the
reference phrase at the default breath, using 1/3-octave bands relative to
the 500 Hz band. The reference values are approximate long-term average speech
spectrum levels for male talkers from Byrne et al. (1994).

| Band | Previous | Current | Reference |
| --- | ---: | ---: | ---: |
| 250 Hz | +16 dB | +3 dB | −1 dB |
| 1 kHz | −14 dB | −6 dB | −6 dB |
| 2 kHz | −10 dB | −11 dB | −12 dB |
| 3.15 kHz | −8 dB | −16 dB | −16 dB |
| 4 kHz | −9 dB | −24 dB | −18 dB |

Above 4 kHz the current mix sits below the reference because consonant
noise remains deliberately soft.

## References

- Praat's KlattGrid models aspiration, breathiness, and frication separately.
  Its phonation code shows low-pass shaping of aspiration and modulation of
  breathiness during the glottal open phase. [Praat KlattGrid source](https://github.com/praat/praat/blob/master/dwtools/KlattGrid.cpp)
- eSpeak NG's Klatt renderer uses shaped noise and independent amplitudes
  for aspiration and frication. [eSpeak NG Klatt renderer](https://github.com/espeak-ng/espeak-ng/blob/master/src/libespeak-ng/klatt.c)
- Dennis H. Klatt's original cascade/parallel synthesizer paper describes
  the architecture underlying these source and filter choices. [Klatt, 1980](https://doi.org/10.1121/1.383940)
