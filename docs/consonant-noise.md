# Breath and consonant noise

jTalk renders voicing and consonant turbulence as separate signals. Dialogue and
MusicXML singing share this part of the renderer. The pronunciation planner
places `W IH DH` in “with” by default because the first entry in the bundled
dictionary uses the voiced dental fricative. The alternative `TH` is the
unvoiced sound in “thin.” Override the pronunciation to pick the alternate
without changing how either phone is synthesized.

## Signal paths

### Voicing and breath

The glottal pulse generates periodic energy at the planned pitch. A small dry
component goes straight to the mix, and three resonators shape the rest into
vowel and consonant formants. At a boundary between two voiced phones,
both sides meet at the average of their voicing gains. Smooth ramps link each
phone's own gain to that boundary value. Boundaries with silence or unvoiced
phones fade to zero.

A separate breath contribution enters the three resonators. It starts with
seeded white noise and passes through a one-pole low-pass at 1.4 kHz, using
`1 - exp(-2π × 1400 / sample_rate)` as the smoothing coefficient. The
smoothed signal is scaled by the glottal pulse and `Voice::breath`. That
concentrates the airflow in the open part of the cycle and keeps it out of
the dry component. The CLI and library default to 0.035. The browser and Windows app start
at 73% in their Default presets. The robot preset uses zero.

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
| S | 4,750 Hz | 1,100 Hz | 0.28 | 0 |
| T | 3,550 Hz | 1,200 Hz | 0.34 | 0 |
| TH | 2,650 Hz | 1,200 Hz | 0.16 | 0 |
| DH | 2,250 Hz | 1,100 Hz | 0.09 | 0.52 |

These are jTalk tuning values at a formant scale of 1.0. The higher center
frequency on S keeps it distinct from the dental fricatives. DH gets most
of its energy from voicing. TH has a little more noise to preserve the
unvoiced articulation. Other fricatives retain their own bands and levels.

Steady fricatives have a rounded attack and release. Oral stops
(`P B T D K G`) keep their noise at zero through closure and release it
during the final 12 ms, or the final 40% if the phone is shorter than 30 ms.
Voiced stops can carry quiet voicing during that closure. Affricates
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

## References

- Praat's KlattGrid models aspiration, breathiness, and frication separately.
  Its phonation code shows low-pass shaping of aspiration and modulation of
  breathiness during the glottal open phase. [Praat KlattGrid source](https://github.com/praat/praat/blob/master/dwtools/KlattGrid.cpp)
- eSpeak NG's Klatt renderer uses shaped noise and independent amplitudes
  for aspiration and frication. [eSpeak NG Klatt renderer](https://github.com/espeak-ng/espeak-ng/blob/master/src/libespeak-ng/klatt.c)
- Dennis H. Klatt's original cascade/parallel synthesizer paper describes
  the architecture underlying these source and filter choices. [Klatt, 1980](https://doi.org/10.1121/1.383940)
