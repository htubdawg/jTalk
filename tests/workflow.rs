use jtalk::{
    PHONE_SYMBOLS, PHONE_VARIANTS, Phone, Plan, Pronunciations, SongOptions, Voice, plan_song,
    plan_speech, pronounce, render, render_singing, song_lines, write_wav,
};

#[test]
fn musicxml_melisma_is_one_syllable_with_three_pitches() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/stay-with-me.musicxml"
    );
    let lines = song_lines(path).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].part_id, "P1");
    assert_eq!(lines[0].part_name, "Voice");
    assert_eq!(lines[0].verse, "1");
    let plan = plan_song(path, &lines[0].options(), &Pronunciations::default()).unwrap();

    let stay = &plan.cues[0];
    assert_eq!(stay.text, "Stay");
    assert!((stay.start - 0.3).abs() < 1e-8);
    assert!((stay.end - 1.8).abs() < 1e-8);
    assert!((plan.duration - 4.8).abs() < 1e-8);
    let stay_phones: Vec<_> = plan
        .phones
        .iter()
        .filter(|phone| phone.end <= stay.end + 1e-8)
        .map(|phone| phone.symbol.as_str())
        .collect();
    assert_eq!(stay_phones, ["S", "T", "EY"]);
    // Each note holds until 50 ms before the next, then glides into it.
    for (seconds, expected_hz) in [
        (0.3, 261.626),
        (0.85, 261.626),
        (0.9, 293.665),
        (1.15, 293.665),
        (1.2, 329.628),
    ] {
        assert!(plan.pitch.iter().any(|target| {
            (target.time - seconds).abs() < 1e-8 && (target.hz - expected_hz).abs() < 0.001
        }));
    }

    let audio = render_singing(&plan, Voice::robot(), 16_000).unwrap();
    assert_eq!(audio.samples.len(), 76_800);
    // The S of "Stay" starts just ahead of the beat at 0.3 s.
    let onset = plan.phones[0].start;
    assert!(onset < 0.3 && onset > 0.2, "{onset}");
    let silent = (onset * 16_000.0) as usize;
    assert!(audio.samples[..silent].iter().all(|sample| *sample == 0.0));
    let vowel = &audio.samples[9120..12960];
    let harmonic = |hz: f64| {
        let (sin, cos) = vowel
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |(sin, cos), (i, sample)| {
                let angle = std::f64::consts::TAU * hz * i as f64 / 16_000.0;
                let window =
                    0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / vowel.len() as f64).cos();
                let value = f64::from(*sample) * window;
                (sin + value * angle.sin(), cos + value * angle.cos())
            });
        sin * sin + cos * cos
    };
    assert!(harmonic(523.251) > 1.8 * harmonic(261.626));
    for (seconds, hz) in [(0.65, 261.626), (1.0, 293.665), (1.4, 329.628)] {
        let start = (seconds * 16_000.0) as usize;
        // Compare with the signal one exact period later, interpolated.
        let period = 16_000.0_f64 / hz;
        let (whole, fraction) = (period as usize, (period.fract()) as f32);
        let error: f32 = (start..start + 800)
            .map(|i| {
                let later = audio.samples[i + whole] * (1.0 - fraction)
                    + audio.samples[i + whole + 1] * fraction;
                (audio.samples[i] - later).powi(2)
            })
            .sum();
        let power: f32 = audio.samples[start..start + 800]
            .iter()
            .map(|value| value * value)
            .sum();
        assert!(power > 0.01 && error < power * 0.1, "{hz} Hz");
    }
}

#[test]
fn dialogue_uses_a_pronunciation_override_and_writes_pcm() {
    let mut custom = Pronunciations::default();
    custom.insert("jtalk", "JH EY1 T AO1 K").unwrap();
    assert_eq!(
        pronounce("jTalk", &custom).unwrap(),
        ["JH", "EY", "T", "AO", "K"]
    );
    let voice = Voice::default();
    let plan = plan_speech("Can you hear jTalk?", voice, 1.0, &custom).unwrap();
    assert_eq!(plan.cues.last().unwrap().text, "jTalk");
    let last_two = &plan.pitch[plan.pitch.len() - 2..];
    assert!(last_two[1].hz > last_two[0].hz);

    let audio = render(&plan, voice, 24_000).unwrap();
    assert_eq!(audio.cues, plan.cues);
    assert!(audio.samples.iter().any(|sample| sample.abs() > 0.02));
    let mut wav = Vec::new();
    write_wav(&audio, &mut wav).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(wav.len(), 44 + 2 * audio.samples.len());
}

#[test]
fn each_reference_phone_can_be_rendered() {
    assert_eq!(PHONE_SYMBOLS.len(), 44);
    for &symbol in PHONE_SYMBOLS.iter().chain(PHONE_VARIANTS) {
        let plan = Plan {
            phones: vec![Phone {
                symbol: symbol.to_owned(),
                start: 0.0,
                end: 0.12,
            }],
            duration: 0.12,
            ..Plan::default()
        };
        let audio = render(&plan, Voice::default(), 16_000).unwrap();
        assert!(audio.samples.iter().all(|sample| sample.is_finite()));
        assert!(
            audio.samples.iter().any(|sample| sample.abs() > 0.01),
            "{symbol}"
        );
    }
}

#[test]
fn stay_has_a_soft_s_and_a_short_t_release() {
    let plan = Plan {
        phones: [("S", 0.0, 0.075), ("T", 0.075, 0.135), ("EY", 0.135, 0.36)]
            .into_iter()
            .map(|(symbol, start, end)| Phone {
                symbol: symbol.to_owned(),
                start,
                end,
            })
            .collect(),
        duration: 0.36,
        ..Plan::default()
    };
    for rate in [16_000, 48_000] {
        let audio = render(&plan, Voice::robot(), rate).unwrap();
        let rms = |start: f64, end: f64| {
            let first = (start * f64::from(rate)).round() as usize;
            let last = (end * f64::from(rate)).round() as usize;
            let samples = &audio.samples[first..last];
            (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
                .sqrt()
        };
        let vowel = rms(0.165, 0.225);
        let s = rms(0.012, 0.062);
        let t_release = rms(0.123, 0.135);
        assert!(
            (0.1..0.7).contains(&(s / vowel)),
            "{rate} Hz: S {s}, vowel {vowel}"
        );
        assert_eq!(rms(0.075, 0.123), 0.0, "{rate} Hz: T closure");
        assert!(
            (0.08..0.6).contains(&(t_release / vowel)),
            "{rate} Hz: T {t_release}, vowel {vowel}"
        );
    }
}

#[test]
fn sung_with_has_a_voiced_dental_without_strong_high_frequency_ripple() {
    let plan = plan_song(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/stay-with-me.musicxml"
        ),
        &SongOptions::default(),
        &Pronunciations::default(),
    )
    .unwrap();
    let with = &plan.cues[1];
    let dental = plan
        .phones
        .iter()
        .find(|phone| phone.symbol == "DH")
        .unwrap();
    assert_eq!(with.text, "with");
    // The M of "me" is sung slightly ahead of its note, shortening the DH.
    let m = plan
        .phones
        .iter()
        .find(|phone| phone.symbol == "M")
        .unwrap();
    assert!((dental.end - m.start).abs() < 1e-8);
    assert!(with.end - dental.end < 0.06 && dental.end < with.end);
    let audio = render_singing(&plan, Voice::robot(), 48_000).unwrap();
    let start = ((dental.start + 0.005) * 48_000.0).round() as usize;
    let end = ((dental.end - 0.010) * 48_000.0).round() as usize;
    let samples = &audio.samples[start..end];
    let power: f32 = samples.iter().map(|sample| sample.powi(2)).sum();
    let ripple: f32 = samples
        .windows(3)
        .map(|w| (w[0] - 2.0 * w[1] + w[2]).powi(2))
        .sum();
    assert!(power / samples.len() as f32 > 0.002);
    // A second difference strongly weights the high-frequency portion.
    assert!(ripple / power < 0.003);
}

#[test]
fn default_dialogue_breath_is_present_below_voicing() {
    // A held vowel at a pitch with an integer period: without breath it
    // repeats exactly, and breath adds a small aperiodic component.
    let plan = Plan {
        phones: vec![Phone {
            symbol: "AA".to_owned(),
            start: 0.0,
            end: 0.5,
        }],
        duration: 0.5,
        ..Plan::default()
    };
    let aperiodic = |breath: f64| {
        let voice = Voice {
            speech_hz: 125.0,
            jitter_cents: 0.0,
            breath,
            ..Voice::default()
        };
        let audio = render(&plan, voice, 48_000).unwrap();
        let samples = &audio.samples[4800..19_200];
        let error: f64 = samples
            .iter()
            .zip(&samples[384..])
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum();
        let power: f64 = samples.iter().map(|s| f64::from(s * s)).sum();
        error / power
    };
    let dry = aperiodic(0.0);
    let default = aperiodic(Voice::default().breath);
    let desktop = aperiodic(0.73);
    assert!(dry < 1e-6, "{dry}");
    assert!(default > 1e-7 && default < 1e-3, "{default}");
    assert!(desktop > default && desktop < 0.1, "{desktop}");
}

#[test]
fn sibilant_preserves_its_band_without_a_strong_high_frequency_tail() {
    let rate = 48_000;
    let voice = Voice {
        breath: 0.35,
        ..Voice::default()
    };
    let plan = plan_speech("Stay", voice, 1.0, &Pronunciations::default()).unwrap();
    let audio = render(&plan, voice, rate).unwrap();
    let samples = &audio.samples[576..2880];
    let band_power = |frequencies: std::ops::Range<usize>| {
        frequencies
            .step_by(100)
            .map(|hz| {
                let angle = std::f64::consts::TAU * hz as f64 / f64::from(rate);
                let coefficient = 2.0 * angle.cos();
                let mut previous = 0.0;
                let mut older = 0.0;
                for (i, &sample) in samples.iter().enumerate() {
                    let t = i as f64 / (samples.len() - 1) as f64;
                    let window = 0.5 - 0.5 * (std::f64::consts::TAU * t).cos();
                    let next = f64::from(sample) * window + coefficient * previous - older;
                    older = previous;
                    previous = next;
                }
                previous * previous + older * older - coefficient * previous * older
            })
            .sum::<f64>()
    };
    let articulation = band_power(3500..6000);
    let fizz = band_power(8000..20_000);
    assert!(articulation > 1.0);
    assert!(fizz / articulation < 0.01);
}

#[test]
fn unknown_words_are_pronounced_and_overrides_take_precedence() {
    let mut pronunciations = Pronunciations::default();
    assert_eq!(
        pronounce("Vexmora", &pronunciations).unwrap(),
        ["V", "EH", "K", "S", "M", "AO", "R", "AX"]
    );
    assert_eq!(
        pronounce("RGB", &pronunciations).unwrap(),
        ["AA", "R", "JH", "IY", "B", "IY"]
    );
    assert_eq!(
        pronounce("B7", &pronunciations).unwrap(),
        ["B", "IY", "S", "EH", "V", "AX", "N"]
    );
    let plan = plan_speech(
        "Welcome, Vexmora. Your code is B7.",
        Voice::default(),
        1.0,
        &pronunciations,
    )
    .unwrap();
    assert_eq!(plan.cues[1].text, "Vexmora");
    assert_eq!(plan.cues[5].text, "B7");
    let audio = render(&plan, Voice::default(), 16_000).unwrap();
    assert!(audio.samples.iter().any(|sample| sample.abs() > 0.02));

    pronunciations
        .insert("Vexmora", "V EH1 K S M OW2 R AX")
        .unwrap();
    assert_eq!(
        pronounce("vexmora", &pronunciations).unwrap(),
        ["V", "EH", "K", "S", "M", "OW", "R", "AX"]
    );
}

#[test]
fn reference_phrase_renders_at_the_desktop_default_breath() {
    let phrase = include_str!("../examples/reference-phrase.txt").trim();
    let voice = Voice {
        breath: 0.73,
        ..Voice::default()
    };
    let plan = plan_speech(phrase, voice, 1.0, &Pronunciations::default()).unwrap();
    let words: Vec<_> = plan.cues.iter().map(|cue| cue.text.as_str()).collect();
    assert_eq!(words.len(), 27);
    assert_eq!(words.first(), Some(&"The"));
    assert_eq!(words.last(), Some(&"wanted"));
    assert_eq!(words.iter().filter(|&&word| word == "loch").count(), 1);
    for symbol in ["ZH", "SH", "TH", "DH", "NG"] {
        assert!(
            plan.phones.iter().any(|phone| phone.symbol == symbol),
            "{symbol}"
        );
    }
    let audio = render(&plan, voice, 24_000).unwrap();
    assert!(audio.samples.iter().all(|sample| sample.abs() < 1.0));
    assert!(audio.samples.iter().any(|sample| sample.abs() > 0.02));
}

#[test]
fn voiceless_stops_release_into_aspiration() {
    let syllable = |stop: &str| Plan {
        phones: [(stop, 0.0, 0.06), ("IY", 0.06, 0.3)]
            .into_iter()
            .map(|(symbol, start, end)| Phone {
                symbol: symbol.to_owned(),
                start,
                end,
            })
            .collect(),
        duration: 0.3,
        ..Plan::default()
    };
    // Aperiodic share of the signal just after the release, at a pitch with
    // a whole-sample period of 384 samples.
    let aperiodic = |stop: &str| {
        let voice = Voice {
            speech_hz: 125.0,
            ..Voice::robot()
        };
        let audio = render(&syllable(stop), voice, 48_000).unwrap();
        let samples = &audio.samples[3360..4320];
        let later = &audio.samples[3360 + 384..4320 + 384];
        let power: f32 = samples.iter().map(|s| s * s).sum();
        let error: f32 = samples
            .iter()
            .zip(later)
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        error / power
    };
    let (tea, dee) = (aperiodic("T"), aperiodic("D"));
    assert!(tea > 0.25 && dee < 0.1, "T {tea}, D {dee}");
}
