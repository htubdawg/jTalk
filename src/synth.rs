use std::f64::consts::TAU;
use std::io::Write;

use crate::{Cue, PHONE_SYMBOLS, PHONE_VARIANTS, Plan, Result, invalid};

/// Independent source pitch and vocal-tract controls. Speech pitch is also
/// the fallback for custom plans without pitch targets. Score pitch is absolute.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub speech_hz: f64,
    pub formant_scale: f64,
    pub breath: f64,
    pub jitter_cents: f64,
    pub vibrato_cents: f64,
    pub vibrato_hz: f64,
    pub gain: f64,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            speech_hz: 135.0,
            formant_scale: 1.0,
            breath: 0.08,
            jitter_cents: 3.0,
            vibrato_cents: 0.0,
            vibrato_hz: 5.0,
            gain: 0.8,
        }
    }
}

impl Voice {
    pub fn robot() -> Self {
        Self {
            breath: 0.0,
            jitter_cents: 0.0,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Audio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
    pub cues: Vec<Cue>,
}

#[derive(Clone, Copy)]
enum NoiseEnvelope {
    Steady,
    Ramped,
    StopRelease,
}

#[derive(Clone, Copy)]
struct Profile {
    first: [f64; 3],
    last: [f64; 3],
    voice: f64,
    noise: f64,
    band: f64,
    noise_width: f64,
    noise_envelope: NoiseEnvelope,
}

fn profile(symbol: &str) -> Profile {
    let (first, last) = match symbol {
        "AA" => ([730., 1090., 2440.], None),
        "AE" => ([660., 1720., 2410.], None),
        "AH" => ([640., 1190., 2390.], None),
        "AO" => ([570., 840., 2410.], None),
        "AW" => ([720., 1250., 2500.], Some([330., 850., 2200.])),
        "AY" => ([720., 1250., 2500.], Some([310., 2180., 2890.])),
        "EH" => ([530., 1840., 2480.], None),
        "ER" => ([490., 1350., 1680.], None),
        "EY" => ([500., 1900., 2600.], Some([310., 2180., 2890.])),
        "IH" => ([390., 1990., 2550.], None),
        "IY" => ([270., 2290., 3010.], None),
        "OW" => ([480., 920., 2450.], Some([320., 760., 2240.])),
        "OY" => ([570., 840., 2410.], Some([310., 2180., 2890.])),
        "UH" => ([440., 1020., 2240.], None),
        "UW" => ([300., 870., 2240.], None),
        "AX" => ([500., 1470., 2500.], None),
        "LOT" => ([600., 920., 2550.], None),
        "IA" => ([390., 1990., 2550.], Some([500., 1470., 2500.])),
        "EA" => ([530., 1840., 2480.], Some([500., 1470., 2500.])),
        "UA" => ([440., 1020., 2240.], Some([500., 1470., 2500.])),
        "IX" => ([370., 1720., 2480.], None),
        _ => return consonant(symbol),
    };
    Profile {
        first,
        last: last.unwrap_or(first),
        voice: 1.0,
        noise: 0.0,
        band: 0.0,
        noise_width: 0.0,
        noise_envelope: NoiseEnvelope::Steady,
    }
}

fn consonant(symbol: &str) -> Profile {
    use NoiseEnvelope::{Ramped, Steady, StopRelease};

    // Voicing, turbulence and release vary independently by manner of articulation.
    let (first, voice, noise, band, noise_envelope) = match symbol {
        "L" | "EL" => ([360., 1250., 2600.], 0.65, 0.0, 0., Steady),
        "M" | "EM" => ([250., 1050., 2150.], 0.45, 0.0, 0., Steady),
        "N" | "EN" => ([280., 1650., 2350.], 0.48, 0.0, 0., Steady),
        "NG" => ([300., 2050., 2650.], 0.40, 0.0, 0., Steady),
        "R" => ([320., 1300., 1640.], 0.76, 0.0, 0., Steady),
        "W" => ([290., 750., 2400.], 0.76, 0.0, 0., Steady),
        "Y" => ([280., 2260., 3000.], 0.76, 0.0, 0., Steady),
        "B" => ([230., 900., 2300.], 0.22, 0.30, 1050., Ramped),
        "D" => ([230., 1750., 2700.], 0.22, 0.38, 3750., Ramped),
        "G" => ([230., 1950., 2450.], 0.22, 0.35, 2200., Ramped),
        "P" => ([300., 900., 2300.], 0.0, 0.42, 1050., Ramped),
        "T" => ([300., 1750., 2700.], 0.0, 0.40, 3950., StopRelease),
        "K" => ([300., 1950., 2450.], 0.0, 0.52, 2450., Ramped),
        "CH" => ([300., 1750., 2600.], 0.0, 0.86, 3200., Ramped),
        "JH" => ([280., 1750., 2600.], 0.26, 0.78, 3000., Ramped),
        "F" => ([350., 1150., 2450.], 0.0, 0.39, 3000., Steady),
        "V" => ([340., 1150., 2450.], 0.36, 0.36, 2900., Steady),
        "TH" => ([350., 1450., 2600.], 0.0, 0.35, 4700., Steady),
        "DH" => ([350., 1450., 2600.], 0.38, 0.30, 4200., Steady),
        "HH" => ([400., 1450., 2500.], 0.0, 0.31, 1500., Steady),
        "S" => ([350., 1850., 2900.], 0.0, 0.30, 5300., Steady),
        "Z" => ([350., 1850., 2900.], 0.32, 0.75, 5700., Steady),
        "SH" => ([350., 1650., 2450.], 0.0, 0.86, 3250., Steady),
        "ZH" => ([350., 1650., 2450.], 0.35, 0.72, 3050., Steady),
        _ => unreachable!("validated phone"),
    };
    Profile {
        first,
        last: first,
        voice,
        noise,
        band,
        noise_width: match symbol {
            "S" => 1100.0,
            "T" => 1200.0,
            _ => 1750.0,
        },
        noise_envelope,
    }
}

#[derive(Default)]
struct Resonator {
    last: f64,
    previous: f64,
}

impl Resonator {
    fn next(&mut self, input: f64, hz: f64, width: f64, rate: f64) -> f64 {
        let radius = (-0.5 * TAU * width / rate).exp();
        let angle = TAU * hz.min(rate * 0.45) / rate;
        let value = (1.0 - radius) * angle.sin() * input + 2.0 * radius * angle.cos() * self.last
            - radius * radius * self.previous;
        self.previous = self.last;
        self.last = value;
        value
    }
}

fn smooth(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn lerp(from: f64, to: f64, fraction: f64) -> f64 {
    from + (to - from) * fraction
}

fn validate(plan: &Plan, voice: Voice, rate: u32) -> Result<()> {
    if !(16_000..=192_000).contains(&rate) {
        return Err(invalid("sample rate must be between 16000 and 192000 Hz"));
    }
    for (name, value, range) in [
        ("speech pitch", voice.speech_hz, 20.0..=2000.0),
        ("formant scale", voice.formant_scale, 0.5..=2.0),
        ("breath", voice.breath, 0.0..=1.0),
        ("jitter", voice.jitter_cents, 0.0..=100.0),
        ("vibrato depth", voice.vibrato_cents, 0.0..=200.0),
        ("vibrato rate", voice.vibrato_hz, 0.0..=20.0),
        ("gain", voice.gain, 0.0..=4.0),
    ] {
        if !range.contains(&value) {
            return Err(invalid(format!(
                "{name} must be between {} and {}",
                range.start(),
                range.end()
            )));
        }
    }
    if !plan.duration.is_finite() || plan.duration < 0.0 {
        return Err(invalid("plan duration must be finite and nonnegative"));
    }
    let mut end = 0.0;
    for phone in &plan.phones {
        if !PHONE_SYMBOLS.contains(&phone.symbol.as_str())
            && !PHONE_VARIANTS.contains(&phone.symbol.as_str())
        {
            return Err(invalid(format!("unknown phone {}", phone.symbol)));
        }
        if !phone.start.is_finite()
            || !phone.end.is_finite()
            || phone.start < end - 1e-9
            || phone.start < 0.0
            || phone.end <= phone.start
            || phone.end > plan.duration + 1e-9
        {
            return Err(invalid(
                "phones must have positive durations and be ordered without overlap",
            ));
        }
        end = phone.end;
    }
    let mut last = -1.0;
    for pitch in &plan.pitch {
        if !(0.0..=plan.duration).contains(&pitch.time)
            || pitch.time <= last
            || !(8.0..=rate as f64 * 0.4).contains(&pitch.hz)
        {
            return Err(invalid(
                "pitch targets must be strictly ordered and within the plan and sample rate",
            ));
        }
        last = pitch.time;
    }
    Ok(())
}

/// Render mono floating-point PCM. Noise and vocal jitter are deterministic.
pub fn render(plan: &Plan, voice: Voice, sample_rate: u32) -> Result<Audio> {
    validate(plan, voice, sample_rate)?;
    let rate = f64::from(sample_rate);
    let frames = (plan.duration * rate).round() as usize;
    // Bound the in-memory API to a practical allocation.
    if frames > 50_000_000 {
        return Err(invalid("render exceeds the 50 million sample limit"));
    }
    let mut samples = vec![0.0; frames];
    let mut phase = 0.0;
    let mut period_jitter = 0.0;
    let mut seed = 0x82a2_b175_u32;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
    };
    let mut formants = [
        Resonator::default(),
        Resonator::default(),
        Resonator::default(),
    ];
    let mut pitch_index = 0;
    let f0 = |time: f64, index: &mut usize| {
        let Some(first) = plan.pitch.first() else {
            return voice.speech_hz;
        };
        while *index + 1 < plan.pitch.len() && plan.pitch[*index + 1].time <= time {
            *index += 1;
        }
        let left = &plan.pitch[*index];
        match plan.pitch.get(*index + 1) {
            Some(right) if time > left.time => lerp(
                left.hz,
                right.hz,
                (time - left.time) / (right.time - left.time),
            ),
            _ if time < first.time => first.hz,
            _ => left.hz,
        }
    };

    for (index, phone) in plan.phones.iter().enumerate() {
        let shape = profile(&phone.symbol);
        let previous = index
            .checked_sub(1)
            .and_then(|i| plan.phones.get(i))
            .filter(|p| (p.end - phone.start).abs() < 1e-8)
            .map(|p| profile(&p.symbol));
        let next = plan
            .phones
            .get(index + 1)
            .filter(|p| (p.start - phone.end).abs() < 1e-8)
            .map(|p| profile(&p.symbol));
        if previous.is_none() {
            formants
                .iter_mut()
                .for_each(|state| *state = Resonator::default());
        }
        // Consonants have separate noise sources. A preceding fricative
        // should not leave filter energy behind a stop closure.
        let mut turbulence = Resonator::default();
        let start = (phone.start * rate).round() as usize;
        let end = ((phone.end * rate).round() as usize).min(frames);
        let duration = phone.end - phone.start;
        let transition = (duration * 0.3).min(0.025);
        for (frame, sample) in samples.iter_mut().enumerate().take(end).skip(start) {
            let time = frame as f64 / rate;
            let elapsed = (time - phone.start).max(0.0);
            let remaining = (phone.end - time).max(0.0);
            let tail = smooth(
                (elapsed - (duration - 0.16).max(duration * 0.42)) / (duration * 0.42).min(0.14),
            );
            let wobble = voice.vibrato_cents * (TAU * voice.vibrato_hz * time).sin();
            let hz = f0(time, &mut pitch_index) * 2.0_f64.powf((wobble + period_jitter) / 1200.0);
            phase += hz / rate;
            if phase >= 1.0 {
                phase -= 1.0;
                period_jitter = voice.jitter_cents * random();
            }

            // A smooth, asymmetric glottal pulse, with a shorter closing phase.
            let pulse = if phase < 0.40 {
                0.5 - 0.5 * (std::f64::consts::PI * phase / 0.40).cos()
            } else if phase < 0.63 {
                0.5 + 0.5 * (std::f64::consts::PI * (phase - 0.40) / 0.23).cos()
            } else {
                0.0
            };
            let white = random();
            let source = pulse - 0.315 + white * voice.breath;
            let mut voiced = source * 0.16;
            for (band, filter) in formants.iter_mut().enumerate() {
                let target = lerp(shape.first[band], shape.last[band], tail);
                let mut center = target;
                if let Some(prior) = previous {
                    center +=
                        (prior.last[band] - target) * 0.5 * (1.0 - smooth(elapsed / transition));
                }
                if let Some(following) = next {
                    center += (following.first[band] - target)
                        * 0.5
                        * (1.0 - smooth(remaining / transition));
                }
                voiced += [1.3, 1.1, 0.72][band]
                    * filter.next(
                        source,
                        center * voice.formant_scale,
                        [80., 120., 165.][band],
                        rate,
                    );
            }

            let attack = smooth(elapsed / 0.010_f64.min(duration * 0.27));
            let release = smooth(remaining / 0.014_f64.min(duration * 0.27));
            let vocal_attack = if previous.is_some_and(|p| p.voice > 0.4) && shape.voice > 0.4 {
                1.0
            } else {
                attack
            };
            let vocal_release = if next.is_some_and(|p| p.voice > 0.4) && shape.voice > 0.4 {
                1.0
            } else {
                release
            };
            let mut noise = 0.0;
            if shape.noise > 0.0 {
                let envelope = match shape.noise_envelope {
                    NoiseEnvelope::Steady => attack * release,
                    NoiseEnvelope::Ramped => {
                        attack * release * smooth((elapsed / duration - 0.48) / 0.18)
                    }
                    NoiseEnvelope::StopRelease => {
                        // Keep the closure quiet and cap the release even in a long note.
                        let length = 0.012_f64.min(duration * 0.4);
                        let progress = 1.0 - remaining / length;
                        smooth(progress / 0.18) * smooth((1.0 - progress) / 0.82)
                    }
                };
                noise = shape.noise
                    * envelope
                    * 1.8
                    * turbulence.next(
                        white,
                        shape.band * voice.formant_scale,
                        shape.noise_width,
                        rate,
                    );
            }
            let value = voice.gain * (shape.voice * voiced * vocal_attack * vocal_release + noise);
            *sample = value.clamp(-1.0, 1.0) as f32;
        }
    }
    Ok(Audio {
        sample_rate,
        samples,
        cues: plan.cues.clone(),
    })
}

/// Write 16-bit mono PCM WAV to a file, buffer, or standard output.
pub fn write_wav(audio: &Audio, mut writer: impl Write) -> Result<()> {
    let bytes = u32::try_from(audio.samples.len())?
        .checked_mul(2)
        .filter(|bytes| *bytes <= u32::MAX - 36)
        .ok_or_else(|| invalid("audio is too large for RIFF WAV"))?;
    let byte_rate = audio
        .sample_rate
        .checked_mul(2)
        .filter(|_| audio.sample_rate > 0)
        .ok_or_else(|| invalid("invalid WAV sample rate"))?;
    writer.write_all(b"RIFF")?;
    writer.write_all(&(36 + bytes).to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&audio.sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&2u16.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&bytes.to_le_bytes())?;
    let mut buffer = [0; 8192];
    for chunk in audio.samples.chunks(buffer.len() / 2) {
        for (sample, output) in chunk.iter().zip(buffer.as_chunks_mut::<2>().0) {
            *output = ((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes();
        }
        writer.write_all(&buffer[..chunk.len() * 2])?;
    }
    writer.flush()?;
    Ok(())
}
