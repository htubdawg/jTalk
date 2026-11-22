use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::io::Write;

use crate::{Cue, PHONE_SYMBOLS, PHONE_VARIANTS, Plan, Result, invalid};

/// Independent source pitch and vocal-tract controls. Speech pitch is also
/// the fallback for custom plans without pitch targets. Score pitch is absolute.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub speech_hz: f64,
    pub formant_scale: f64,
    /// Glottal breathiness, independent of consonant turbulence.
    pub breath: f64,
    /// Per-cycle pitch jitter. Also scales shimmer and slow pitch wander.
    pub jitter_cents: f64,
    pub vibrato_cents: f64,
    pub vibrato_hz: f64,
    /// Output peak level. Renders are normalized to a peak of 1.12 × gain
    /// and soft-limited above 0.75.
    pub gain: f64,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            speech_hz: 135.0,
            formant_scale: 1.0,
            breath: 0.035,
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
    Affricate,
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
    /// Aspiration through the vocal tract, used for H.
    aspiration: f64,
    /// Nasal anti-resonance in Hz, or zero for an oral sound.
    nasal: f64,
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
        aspiration: 0.0,
        nasal: 0.0,
    }
}

fn consonant(symbol: &str) -> Profile {
    use NoiseEnvelope::{Affricate, Steady, StopRelease};

    // Voicing, turbulence and release vary independently by manner of articulation.
    let (first, voice, noise, band, noise_envelope) = match symbol {
        "L" | "EL" => ([360., 1250., 2600.], 0.65, 0.0, 0., Steady),
        "M" | "EM" => ([250., 1050., 2150.], 0.27, 0.0, 0., Steady),
        "N" | "EN" => ([280., 1650., 2350.], 0.29, 0.0, 0., Steady),
        "NG" => ([300., 2050., 2650.], 0.24, 0.0, 0., Steady),
        "R" => ([320., 1300., 1640.], 0.76, 0.0, 0., Steady),
        "W" => ([290., 750., 2400.], 0.76, 0.0, 0., Steady),
        "Y" => ([280., 2260., 3000.], 0.76, 0.0, 0., Steady),
        "B" => ([230., 900., 2300.], 0.22, 0.18, 1050., StopRelease),
        "D" => ([230., 1750., 2700.], 0.22, 0.18, 3300., StopRelease),
        "G" => ([230., 1950., 2450.], 0.22, 0.18, 2200., StopRelease),
        "P" => ([300., 900., 2300.], 0.0, 0.23, 1050., StopRelease),
        "T" => ([300., 1750., 2700.], 0.0, 0.25, 3550., StopRelease),
        "K" => ([300., 1950., 2450.], 0.0, 0.25, 2450., StopRelease),
        "CH" => ([300., 1750., 2600.], 0.0, 0.19, 3200., Affricate),
        "JH" => ([280., 1750., 2600.], 0.26, 0.15, 3000., Affricate),
        "F" => ([350., 1150., 2450.], 0.0, 0.15, 2500., Steady),
        "V" => ([340., 1150., 2450.], 0.42, 0.11, 2400., Steady),
        "TH" => ([350., 1450., 2600.], 0.0, 0.115, 2650., Steady),
        "DH" => ([350., 1450., 2600.], 0.52, 0.065, 2250., Steady),
        "HH" => ([400., 1450., 2500.], 0.0, 0.0, 0., Steady),
        "S" => ([350., 1850., 2900.], 0.0, 0.135, 4750., Steady),
        "Z" => ([350., 1850., 2900.], 0.39, 0.115, 4400., Steady),
        "SH" => ([350., 1650., 2450.], 0.0, 0.16, 3250., Steady),
        "ZH" => ([350., 1650., 2450.], 0.41, 0.12, 3050., Steady),
        _ => unreachable!("validated phone"),
    };
    Profile {
        first,
        last: first,
        voice,
        noise,
        band,
        noise_width: match symbol {
            "S" | "Z" | "DH" => 1100.0,
            "TH" | "T" => 1200.0,
            _ => 1750.0,
        },
        noise_envelope,
        aspiration: if symbol == "HH" { 1.0 } else { 0.0 },
        nasal: match symbol {
            "M" | "EM" => 900.0,
            "N" | "EN" => 1600.0,
            "NG" => 3000.0,
            _ => 0.0,
        },
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

/// Render mono floating-point PCM. Noise and vocal irregularity are deterministic.
pub fn render(plan: &Plan, voice: Voice, sample_rate: u32) -> Result<Audio> {
    render_voiced(plan, voice, sample_rate, false)
}

/// Render a sung plan with delayed vibrato, a singer's formant and less breath.
pub fn render_singing(plan: &Plan, voice: Voice, sample_rate: u32) -> Result<Audio> {
    render_voiced(plan, voice, sample_rate, true)
}

/// Klatt-style resonator with unity gain at 0 Hz. In series, these give each
/// vowel its natural relative formant levels without per-vowel gain tables.
#[derive(Clone, Copy, Default)]
struct Pole {
    last: f64,
    previous: f64,
}

/// The matching anti-resonance, used for the nasal zero.
#[derive(Clone, Copy, Default)]
struct Zero {
    last: f64,
    previous: f64,
}

fn coefficients(hz: f64, width: f64, rate: f64) -> (f64, f64, f64) {
    let radius = (-PI * width / rate).exp();
    let b = 2.0 * radius * (TAU * hz.min(rate * 0.45) / rate).cos();
    let c = -radius * radius;
    (1.0 - b - c, b, c)
}

impl Pole {
    fn next(&mut self, input: f64, hz: f64, width: f64, rate: f64) -> f64 {
        let (a, b, c) = coefficients(hz, width, rate);
        let value = a * input + b * self.last + c * self.previous;
        self.previous = self.last;
        self.last = value;
        value
    }
}

impl Zero {
    fn next(&mut self, input: f64, hz: f64, width: f64, rate: f64) -> f64 {
        let (a, b, c) = coefficients(hz, width, rate);
        let value = (input - b * self.last - c * self.previous) / a;
        self.previous = self.last;
        self.last = input;
        value
    }
}

/// Oral sounds place the nasal zero on this pole, so the pair cancels out.
const NASAL_POLE: f64 = 270.0;
/// Calibrates the vocal tract output against consonant turbulence.
const TRACT_GAIN: f64 = 0.02;
/// Level of aspiration noise (H and stop release) entering the vocal tract.
const ASPIRATION: f64 = 1.8;
/// Level of glottal breath noise at a Breath setting of 1.
const BREATH_LEVEL: f64 = 4.2;

/// Formant bandwidths widen with frequency, as in natural speech.
fn bandwidth(band: usize, hz: f64) -> f64 {
    match band {
        0 => 40.0 + 0.06 * hz,
        1 => 60.0 + 0.03 * hz,
        _ => 100.0 + 0.03 * hz,
    }
}

fn is_voiceless_stop(symbol: &str) -> bool {
    matches!(symbol, "P" | "T" | "K")
}

fn render_voiced(plan: &Plan, voice: Voice, sample_rate: u32, singing: bool) -> Result<Audio> {
    validate(plan, voice, sample_rate)?;
    let rate = f64::from(sample_rate);
    let frames = (plan.duration * rate).round() as usize;
    // Bound the in-memory API to a practical allocation.
    if frames > 50_000_000 {
        return Err(invalid("render exceeds the 50 million sample limit"));
    }
    let mut samples = vec![0.0; frames];
    let mut seed = 0x82a2_b175_u32;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
    };
    let follow = |seconds: f64| 1.0 - (-1.0 / (seconds * rate)).exp();
    let formant_follow = follow(0.012);
    let nasal_follow = follow(0.018);
    let pitch_follow = follow(0.015);
    let breath_smoothing = 1.0 - (-TAU * 1400.0 / rate).exp();

    // Vocal tract state carries across phone boundaries within a phrase.
    let mut tract = [Pole::default(); 6];
    let mut nasal_pole = Pole::default();
    let mut nasal_zero = Zero::default();
    let mut formants = [0.0; 3];
    let mut zero_hz = NASAL_POLE;
    let mut breath = 0.0;
    // Glottal source state.
    let mut phase = 0.0;
    let mut previous_flow = 0.0;
    let mut period_jitter = 0.0;
    let mut shimmer = 1.0;
    let mut wander = 0.0;
    let mut smoothed_hz = 0.0;
    let mut held_hz = 0.0;
    let mut held = 0.0;
    let mut vibrato_phase = 0.0;
    let mut vibrato_hz = voice.vibrato_hz;

    // F4 to F6 are fixed. Singing draws F4 and F5 toward F3, clustering
    // energy near 3 kHz into a singer's formant. F6 keeps some air above.
    let upper = if singing {
        [(2950.0, 180.0), (3400.0, 220.0), (4900.0, 700.0)]
    } else {
        [(3300.0, 250.0), (3750.0, 300.0), (4900.0, 700.0)]
    };
    let breath_gain = BREATH_LEVEL * if singing { 1.25 } else { 1.0 };
    // A breathier voice keeps the glottis open longer in each cycle.
    let open_quotient = if singing { 0.5 } else { 0.55 } + 0.2 * voice.breath;
    let opening = open_quotient * 0.65;
    // Pressed voices are brighter than breathy ones.
    let presence = if singing { 1.6 } else { 1.0 } * (1.0 - 0.6 * voice.breath);
    let presence_follow = 1.0 - (-TAU * 1000.0 / rate).exp();
    let mut source_low = 0.0;
    // Jitter also scales shimmer and slow pitch wander; 1.0 at the default.
    let roughness = voice.jitter_cents / 3.0;
    // Fixed-width filters admit a smaller fraction of white noise at higher rates.
    let noise_gain = 1.8 * (rate / 48_000.0).sqrt();
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
        let contiguous = |a: &crate::Phone, b: &crate::Phone| (a.end - b.start).abs() < 1e-8;
        let prior_phone = index
            .checked_sub(1)
            .map(|i| &plan.phones[i])
            .filter(|p| contiguous(p, phone));
        let previous = prior_phone.map(|p| profile(&p.symbol));
        let next = plan
            .phones
            .get(index + 1)
            .filter(|p| contiguous(phone, p))
            .map(|p| profile(&p.symbol));
        let mut shape = profile(&phone.symbol);
        // H is aspiration shaped by the vocal tract of the following sound.
        if shape.aspiration > 0.0
            && let Some(following) = next
        {
            shape.first = following.first;
            shape.last = following.first;
        }
        // A voiceless stop releases into aspiration before voicing begins,
        // except after S, as in "stay".
        let aspirated = shape.voice > 0.0
            && prior_phone.is_some_and(|p| is_voiceless_stop(&p.symbol))
            && !index.checked_sub(2).is_some_and(|i| {
                plan.phones[i].symbol == "S" && contiguous(&plan.phones[i], &plan.phones[i + 1])
            });
        if previous.is_none() {
            tract = [Pole::default(); 6];
            nasal_pole = Pole::default();
            nasal_zero = Zero::default();
            formants = shape.first.map(|hz| hz * voice.formant_scale);
            zero_hz = if shape.nasal > 0.0 {
                shape.nasal
            } else {
                NASAL_POLE
            };
            breath = 0.0;
            previous_flow = 0.0;
            source_low = 0.0;
            smoothed_hz = 0.0;
        }
        // Start each consonant with fresh filters so a fricative cannot
        // leave energy behind a stop closure.
        let mut turbulence = Resonator::default();
        let mut turbulence_skirt = Resonator::default();
        let noise_center = shape.band * voice.formant_scale;
        let start = (phone.start * rate).round() as usize;
        let end = ((phone.end * rate).round() as usize).min(frames);
        let duration = phone.end - phone.start;
        let voice_onset_time = 0.05_f64.min(duration * 0.4);
        for (frame, sample) in samples.iter_mut().enumerate().take(end).skip(start) {
            let time = frame as f64 / rate;
            let elapsed = (time - phone.start).max(0.0);
            let remaining = (phone.end - time).max(0.0);
            let tail = smooth(
                (elapsed - (duration - 0.16).max(duration * 0.42)) / (duration * 0.42).min(0.14),
            );

            // Pitch: the planned contour with rounded corners, then slow
            // wander, per-cycle jitter and vibrato.
            let target_hz = f0(time, &mut pitch_index);
            if (target_hz - held_hz).abs() > target_hz * 1e-9 {
                held = 0.0;
                held_hz = target_hz;
            } else {
                held += 1.0 / rate;
            }
            if smoothed_hz <= 0.0 {
                smoothed_hz = target_hz;
            }
            smoothed_hz *= (target_hz / smoothed_hz).powf(pitch_follow);
            // Sung vibrato settles in once a note has been held for a moment.
            let depth = if singing {
                voice.vibrato_cents * smooth((held - 0.12) / 0.3)
            } else {
                voice.vibrato_cents
            };
            vibrato_phase += vibrato_hz / rate;
            if vibrato_phase >= 1.0 {
                vibrato_phase -= 1.0;
                vibrato_hz = voice.vibrato_hz * (1.0 + 0.06 * roughness.min(1.0) * random());
            }
            // The wander is a slow, mean-reverting walk of about ±9 cents at the default jitter.
            let cents = depth * (TAU * vibrato_phase).sin()
                + period_jitter
                + wander * 2.5 * voice.jitter_cents;
            let hz = smoothed_hz * 2.0_f64.powf(cents / 1200.0);
            phase += hz / rate;
            if phase >= 1.0 {
                phase -= 1.0;
                period_jitter = voice.jitter_cents * random();
                shimmer = 1.0 + 0.04 * roughness * random();
                wander = 0.97 * wander + 0.3 * random();
            }

            // Glottal flow with a smooth opening and an abrupt closure. Its
            // derivative includes lip radiation and keeps the high harmonics
            // that the closure excites.
            let flow = if phase < opening {
                0.5 - 0.5 * (PI * phase / opening).cos()
            } else if phase < open_quotient {
                (FRAC_PI_2 * (phase - opening) / (open_quotient - opening)).cos()
            } else {
                0.0
            };
            let slope = (flow - previous_flow) * rate / hz * shimmer;
            previous_flow = flow;
            // A high shelf lifts the source above about 1 kHz for presence.
            source_low += presence_follow * (slope - source_low);
            let source = slope + presence * (slope - source_low);
            let white = random();
            breath += breath_smoothing * (white - breath);

            let attack = smooth(elapsed / 0.010_f64.min(duration * 0.27));
            let release = smooth(remaining / 0.014_f64.min(duration * 0.27));
            let mut voicing = shape.voice;
            if shape.voice > 0.0 {
                // Share the same midpoint at neighbouring voiced boundaries.
                let attack_floor = previous
                    .filter(|p| p.voice > 0.0)
                    .map_or(0.0, |p| (p.voice + shape.voice) * 0.5);
                let release_floor = next
                    .filter(|p| p.voice > 0.0)
                    .map_or(0.0, |p| (p.voice + shape.voice) * 0.5);
                voicing += (1.0 - attack) * (attack_floor - shape.voice)
                    + (1.0 - release) * (release_floor - shape.voice);
            }
            let mut aspiration = shape.aspiration * attack * release;
            if aspirated {
                let onset = smooth(elapsed / voice_onset_time);
                aspiration += 1.0 - onset;
                voicing *= onset;
            }
            // Breath follows glottal opening and goes through the vocal tract.
            let excitation = voicing * (source + breath * flow * voice.breath * breath_gain)
                + aspiration * ASPIRATION * white;

            // Formants glide toward each target, so consonants bend the
            // neighbouring vowels the way articulators do.
            for (band, formant) in formants.iter_mut().enumerate() {
                let target = lerp(shape.first[band], shape.last[band], tail) * voice.formant_scale;
                *formant += formant_follow * (target - *formant);
            }
            let zero_target = if shape.nasal > 0.0 {
                shape.nasal * voice.formant_scale
            } else {
                NASAL_POLE
            };
            zero_hz += nasal_follow * (zero_target - zero_hz);
            let mut tract_out = nasal_zero.next(excitation, zero_hz, 100.0, rate);
            tract_out = nasal_pole.next(tract_out, NASAL_POLE, 100.0, rate);
            for (band, filter) in tract.iter_mut().enumerate() {
                let (hz, width) = match band {
                    0..3 => (formants[band], bandwidth(band, formants[band])),
                    _ => (upper[band - 3].0 * voice.formant_scale, upper[band - 3].1),
                };
                tract_out = filter.next(tract_out, hz, width, rate);
            }

            let mut noise = 0.0;
            if shape.noise > 0.0 {
                let envelope = match shape.noise_envelope {
                    NoiseEnvelope::Steady => attack * release,
                    NoiseEnvelope::Affricate => {
                        attack * release * smooth((elapsed / duration - 0.48) / 0.18)
                    }
                    NoiseEnvelope::StopRelease => {
                        // Keep the closure quiet and cap the release even in a long note.
                        let length = 0.012_f64.min(duration * 0.4);
                        let progress = 1.0 - remaining / length;
                        smooth(progress / 0.18) * smooth((1.0 - progress) / 0.82)
                    }
                };
                let band = turbulence.next(white, noise_center, shape.noise_width, rate);
                // A broader second stage suppresses the off-band fizz without
                // squeezing the useful consonant band into a narrow peak.
                let shaped =
                    turbulence_skirt.next(band, noise_center, shape.noise_width * 2.0, rate);
                noise = shape.noise * envelope * noise_gain * 1.7 * shaped;
                // Voiced fricatives pulse with the glottal cycle.
                if shape.voice > 0.0 {
                    noise *= 0.5 + 1.6 * flow;
                }
            }
            // Fade the tract at the edges of a phrase so its ringing is not cut off.
            let edge = if previous.is_none() { attack } else { 1.0 }
                * if next.is_none() { release } else { 1.0 };
            *sample = (TRACT_GAIN * tract_out * edge + noise) as f32;
        }
    }
    finish(&mut samples, voice.gain);
    Ok(Audio {
        sample_rate,
        samples,
        cues: plan.cues.clone(),
    })
}

/// Normalize the peak to the voice gain, then round off anything above a
/// knee instead of clipping it.
fn finish(samples: &mut [f32], gain: f64) {
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    if peak <= 0.0 || !peak.is_finite() {
        return;
    }
    let scale = (gain * 1.12) as f32 / peak;
    for sample in samples {
        let value = *sample * scale;
        let magnitude = value.abs();
        *sample = if magnitude <= 0.75 {
            value
        } else {
            value.signum() * (0.75 + 0.25 * ((magnitude - 0.75) / 0.25).tanh())
        };
    }
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
