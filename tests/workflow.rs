use jtalk::{
    PHONE_SYMBOLS, PHONE_VARIANTS, Phone, Plan, Pronunciations, SongOptions, Voice, plan_song,
    plan_speech, pronounce, render, write_wav,
};

#[test]
fn musicxml_melisma_is_one_syllable_with_three_pitches() {
    let plan = plan_song(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/stay-with-me.musicxml"
        ),
        &SongOptions::default(),
        &Pronunciations::default(),
    )
    .unwrap();

    let stay = &plan.cues[0];
    assert_eq!(stay.text, "Stay");
    assert!((stay.start - 0.3).abs() < 1e-8);
    assert!((stay.end - 1.8).abs() < 1e-8);
    assert!((plan.duration - 4.8).abs() < 1e-8);
    let stay_phones: Vec<_> = plan
        .phones
        .iter()
        .filter(|phone| phone.start < stay.end - 1e-8)
        .map(|phone| phone.symbol.as_str())
        .collect();
    assert_eq!(stay_phones, ["S", "T", "EY"]);
    for (seconds, expected_hz) in [(0.3, 261.626), (0.9, 293.665), (1.2, 329.628)] {
        assert!(plan.pitch.iter().any(|target| {
            (target.time - seconds).abs() < 1e-8 && (target.hz - expected_hz).abs() < 0.001
        }));
    }

    let audio = render(&plan, Voice::robot(), 16_000).unwrap();
    assert_eq!(audio.samples.len(), 76_800);
    assert!(audio.samples[..4800].iter().all(|sample| *sample == 0.0));
    for (seconds, hz) in [(0.65, 261.626), (1.0, 293.665), (1.4, 329.628)] {
        let start = (seconds * 16_000.0) as usize;
        let period = (16_000.0_f64 / hz).round() as usize;
        let error: f32 = (start..start + 800)
            .map(|i| (audio.samples[i] - audio.samples[i + period]).powi(2))
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
    assert!((dental.end - with.end).abs() < 1e-8);
    let audio = render(&plan, Voice::robot(), 48_000).unwrap();
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
    let voice = Voice::default();
    let dry_voice = Voice {
        breath: 0.0,
        ..voice
    };
    let plan = plan_speech(
        "Stay with me. Can you hear me?",
        voice,
        1.0,
        &Pronunciations::default(),
    )
    .unwrap();
    let wet = render(&plan, voice, 48_000).unwrap();
    let dry = render(&plan, dry_voice, 48_000).unwrap();
    let breath_power: f32 = wet
        .samples
        .iter()
        .zip(&dry.samples)
        .map(|(a, b)| (a - b).powi(2))
        .sum();
    let voice_power: f32 = dry.samples.iter().map(|sample| sample.powi(2)).sum();
    assert!((1e-7..1e-4).contains(&(breath_power / voice_power)));
}
