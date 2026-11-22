use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use roxmltree::{Document, Node};

use crate::english::{Pronunciations, Sound, distribute, lyric_syllables, weight};
use crate::{Cue, Pitch, Plan, Result, invalid, midi_hz};

const MAX_XML_BYTES: u64 = 16 * 1024 * 1024;
const EPSILON: f64 = 1e-8;

#[derive(Clone, Debug, Default)]
pub struct SongOptions {
    /// MusicXML part ID, or the exact displayed part name.
    pub part: Option<String>,
    pub verse: Option<String>,
    pub voice: Option<String>,
    pub staff: Option<String>,
    /// Quarter notes per minute when the score has no initial tempo.
    pub default_bpm: Option<f64>,
}

type Element<'a, 'input> = Node<'a, 'input>;

fn named(node: Element<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn child<'a, 'i>(node: Element<'a, 'i>, name: &str) -> Option<Element<'a, 'i>> {
    node.children().find(|&node| named(node, name))
}

fn text<'a>(node: Element<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name)
        .and_then(|node| node.text())
        .map(str::trim)
}

fn number(value: &str, label: &str) -> Result<f64> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| invalid(format!("invalid MusicXML {label}: {value}")))?;
    if !parsed.is_finite() {
        return Err(invalid(format!("invalid MusicXML {label}: {value}")));
    }
    Ok(parsed)
}

fn read_xml(reader: impl Read, maximum: u64) -> Result<String> {
    let mut bytes = Vec::new();
    reader.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("MusicXML document exceeds the size limit"));
    }
    Ok(String::from_utf8(bytes)?)
}

fn score_xml(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mxl"))
    {
        let mut archive = zip::ZipArchive::new(file)?;
        let manifest = read_xml(archive.by_name("META-INF/container.xml")?, 65_536)?;
        let document = Document::parse(&manifest)?;
        let rootfile = document
            .descendants()
            .find(|&node| named(node, "rootfile"))
            .and_then(|node| node.attribute("full-path"))
            .ok_or_else(|| invalid("MXL container has no score rootfile"))?;
        return read_xml(archive.by_name(rootfile)?, MAX_XML_BYTES);
    }
    read_xml(file, MAX_XML_BYTES)
}

#[derive(Clone, Copy)]
struct Tempo {
    beat: f64,
    bpm: f64,
}

fn direction_tempo(direction: Element<'_, '_>) -> Result<Option<f64>> {
    if let Some(value) = child(direction, "sound").and_then(|sound| sound.attribute("tempo")) {
        return Ok(Some(number(value, "tempo")?));
    }
    let Some(metronome) = direction
        .children()
        .filter(|&node| named(node, "direction-type"))
        .find_map(|node| child(node, "metronome"))
    else {
        return Ok(None);
    };
    let Some(value) = text(metronome, "per-minute") else {
        return Ok(None);
    };
    let units = match text(metronome, "beat-unit") {
        Some("whole") => 4.0,
        Some("half") => 2.0,
        Some("quarter") => 1.0,
        Some("eighth") => 0.5,
        Some("16th") => 0.25,
        Some("32nd") => 0.125,
        _ => return Err(invalid("unsupported MusicXML metronome beat unit")),
    };
    let dots = metronome
        .children()
        .filter(|&node| named(node, "beat-unit-dot"))
        .count();
    Ok(Some(
        number(value, "tempo")? * units * (2.0 - 2.0_f64.powf(-(dots as f64))),
    ))
}

fn verse_id<'a>(lyric: Element<'a, '_>) -> &'a str {
    lyric
        .attribute("number")
        .or_else(|| lyric.attribute("name"))
        .unwrap_or("1")
}

/// One lyric-bearing part, staff, voice, and verse in a MusicXML score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongLine {
    pub part_id: String,
    pub part_name: String,
    pub voice: String,
    pub staff: String,
    pub verse: String,
}

impl SongLine {
    /// Select this exact line when planning a vocal render.
    pub fn options(&self) -> SongOptions {
        SongOptions {
            part: Some(self.part_id.clone()),
            verse: Some(self.verse.clone()),
            voice: Some(self.voice.clone()),
            staff: Some(self.staff.clone()),
            ..SongOptions::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LyricLine<'a, 'i> {
    part: Element<'a, 'i>,
    id: &'a str,
    name: &'a str,
    voice: &'a str,
    staff: &'a str,
    verse: &'a str,
}

fn lyric_lines<'a, 'i>(root: Element<'a, 'i>) -> Vec<LyricLine<'a, 'i>> {
    let names = child(root, "part-list");
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for part in root.children().filter(|&node| named(node, "part")) {
        let id = part.attribute("id").unwrap_or("");
        let name = names
            .and_then(|list| {
                list.children()
                    .find(|node| node.attribute("id") == Some(id))
            })
            .and_then(|node| text(node, "part-name"))
            .filter(|name| !name.is_empty())
            .unwrap_or(id);
        for note in part.descendants().filter(|&node| named(node, "note")) {
            let voice = text(note, "voice").unwrap_or("1");
            let staff = text(note, "staff").unwrap_or("1");
            for lyric in note
                .children()
                .filter(|&node| named(node, "lyric") && child(node, "text").is_some())
            {
                let line = LyricLine {
                    part,
                    id,
                    name,
                    voice,
                    staff,
                    verse: verse_id(lyric),
                };
                if seen.insert(line) {
                    lines.push(line);
                }
            }
        }
    }
    lines
}

/// List the vocal lines available for selection in a MusicXML or MXL score.
pub fn song_lines(path: impl AsRef<Path>) -> Result<Vec<SongLine>> {
    let xml = score_xml(path.as_ref())?;
    let document = score_document(&xml)?;
    Ok(lyric_lines(document.root_element())
        .into_iter()
        .map(|line| SongLine {
            part_id: line.id.to_owned(),
            part_name: line.name.to_owned(),
            voice: line.voice.to_owned(),
            staff: line.staff.to_owned(),
            verse: line.verse.to_owned(),
        })
        .collect())
}

fn select<'a, 'i>(root: Element<'a, 'i>, options: &SongOptions) -> Result<LyricLine<'a, 'i>> {
    let lines = lyric_lines(root);
    let part_is_id = options
        .part
        .as_deref()
        .is_some_and(|part| lines.iter().any(|line| line.id == part));
    let mut candidates = lines.into_iter().filter(|line| {
        options.part.as_deref().is_none_or(|part| {
            if part_is_id {
                part == line.id
            } else {
                part == line.name
            }
        }) && options
            .voice
            .as_deref()
            .is_none_or(|voice| voice == line.voice)
            && options
                .staff
                .as_deref()
                .is_none_or(|staff| staff == line.staff)
            && options
                .verse
                .as_deref()
                .is_none_or(|verse| verse == line.verse)
    });
    let Some(mut selected) = candidates.next() else {
        return Err(invalid(
            "no embedded lyrics match the selected MusicXML part, staff, voice and verse",
        ));
    };
    for line in candidates {
        if (line.part, line.voice, line.staff) != (selected.part, selected.voice, selected.staff) {
            return Err(invalid(
                "multiple lyric-bearing lines; select a MusicXML part, staff or voice",
            ));
        }
        if line.verse == "1" {
            selected = line;
        }
    }
    Ok(selected)
}

#[derive(Clone)]
struct Lyric {
    text: String,
    syllabic: String,
    extend: Option<String>,
}

impl Lyric {
    fn from_xml(note: Element<'_, '_>, verse: &str) -> Option<Self> {
        let node = note
            .children()
            .find(|&node| named(node, "lyric") && verse_id(node) == verse)?;
        let text = node
            .children()
            .filter(|&node| named(node, "text"))
            .filter_map(|node| node.text())
            .collect::<Vec<_>>()
            .join(" ");
        Some(Self {
            text,
            syllabic: self::text(node, "syllabic").unwrap_or("single").to_owned(),
            extend: child(node, "extend")
                .map(|el| el.attribute("type").unwrap_or("start").to_owned()),
        })
    }
}

struct Note {
    start: f64,
    end: f64,
    midi: f64,
    lyric: Option<Lyric>,
    tied_from_previous: bool,
}

#[derive(Default)]
struct Line {
    notes: Vec<Note>,
    tempos: Vec<Tempo>,
    beats: f64,
}

fn pitch(note: Element<'_, '_>, transpose: f64) -> Result<f64> {
    let pitch = child(note, "pitch")
        .ok_or_else(|| invalid("vocal notes must have a pitched MusicXML note"))?;
    let step = match text(pitch, "step") {
        Some("C") => 0.0,
        Some("D") => 2.0,
        Some("E") => 4.0,
        Some("F") => 5.0,
        Some("G") => 7.0,
        Some("A") => 9.0,
        Some("B") => 11.0,
        _ => return Err(invalid("missing or invalid MusicXML pitch step")),
    };
    let octave = number(
        text(pitch, "octave").ok_or_else(|| invalid("missing MusicXML octave"))?,
        "octave",
    )?;
    let alter = number(text(pitch, "alter").unwrap_or("0"), "pitch alteration")?;
    let midi = (octave + 1.0) * 12.0 + step + alter + transpose;
    if !(0.0..=127.0).contains(&midi) {
        return Err(invalid("MusicXML pitch is outside MIDI note range"));
    }
    Ok(midi)
}

fn parse_line(part: Element<'_, '_>, selection: Option<(&str, &str, &str)>) -> Result<Line> {
    let mut line = Line::default();
    let mut divisions = 1.0;
    let mut transpose = 0.0;
    for measure in part.children().filter(|&node| named(node, "measure")) {
        let mut cursor = line.beats;
        let mut furthest = cursor;
        let mut last_start = cursor;
        for node in measure.children().filter(Node::is_element) {
            match node.tag_name().name() {
                "attributes" => {
                    if let Some(value) = text(node, "divisions") {
                        divisions = number(value, "divisions")?;
                        if divisions <= 0.0 {
                            return Err(invalid("MusicXML divisions must be positive"));
                        }
                    }
                    if let Some(change) =
                        node.children()
                            .filter(|&el| named(el, "transpose"))
                            .find(|el| {
                                el.attribute("number").is_none_or(|n| {
                                    selection.is_some_and(|(_, staff, _)| n == staff)
                                })
                            })
                    {
                        transpose =
                            number(text(change, "chromatic").unwrap_or("0"), "transposition")?
                                + 12.0
                                    * number(
                                        text(change, "octave-change").unwrap_or("0"),
                                        "octave change",
                                    )?;
                    }
                }
                "direction" => {
                    if let Some(bpm) = direction_tempo(node)? {
                        // An offset on a direction applies to playback unless it explicitly says otherwise.
                        let offset = child(node, "offset")
                            .filter(|el| el.attribute("sound") != Some("no"))
                            .and_then(|el| el.text())
                            .unwrap_or("0");
                        let beat =
                            (cursor + number(offset, "direction offset")? / divisions).max(0.0);
                        line.tempos.push(Tempo { beat, bpm });
                    }
                }
                "sound" => {
                    if let Some(value) = node.attribute("tempo") {
                        line.tempos.push(Tempo {
                            beat: cursor,
                            bpm: number(value, "tempo")?,
                        });
                    }
                }
                "note" | "backup" | "forward" => {
                    let tag = node.tag_name().name();
                    if tag == "note" && child(node, "grace").is_some() {
                        continue;
                    }
                    let value = text(node, "duration")
                        .ok_or_else(|| invalid(format!("missing MusicXML {tag} duration")))?;
                    let duration = number(value, "duration")? / divisions;
                    if !duration.is_finite() || duration <= 0.0 {
                        return Err(invalid("MusicXML duration must be positive"));
                    }
                    if tag == "backup" {
                        cursor -= duration;
                        if cursor < line.beats - EPSILON {
                            return Err(invalid("MusicXML backup crosses the measure start"));
                        }
                        cursor = cursor.max(line.beats);
                        continue;
                    }
                    let chord = child(node, "chord").is_some();
                    let start = if chord { last_start } else { cursor };
                    let end = start + duration;
                    if !chord {
                        cursor = end;
                        last_start = start;
                    }
                    furthest = furthest.max(end);
                    let Some((voice, staff, verse)) = selection else {
                        continue;
                    };
                    if tag != "note"
                        || child(node, "rest").is_some()
                        || child(node, "cue").is_some()
                        || text(node, "voice").unwrap_or("1") != voice
                        || text(node, "staff").unwrap_or("1") != staff
                    {
                        continue;
                    }
                    if chord {
                        return Err(invalid("selected MusicXML vocal line has a chord"));
                    }
                    let tied_from_previous = node
                        .children()
                        .any(|el| named(el, "tie") && el.attribute("type") == Some("stop"))
                        || child(node, "notations").is_some_and(|notations| {
                            notations.children().any(|el| {
                                named(el, "tied")
                                    && matches!(el.attribute("type"), Some("stop" | "continue"))
                            })
                        });
                    line.notes.push(Note {
                        start,
                        end,
                        midi: pitch(node, transpose)?,
                        lyric: Lyric::from_xml(node, verse),
                        tied_from_previous,
                    });
                }
                _ => {}
            }
        }
        line.beats = furthest;
    }
    Ok(line)
}

struct Clock(Vec<(f64, f64, f64)>);

impl Clock {
    fn new(mut changes: Vec<Tempo>, default: f64) -> Result<Self> {
        changes.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let mut points = vec![(0.0, 0.0, 60.0 / default)];
        for Tempo { beat, bpm } in changes {
            if bpm <= 0.0 || !beat.is_finite() || !bpm.is_finite() {
                return Err(invalid(
                    "MusicXML tempo and position must be finite with positive tempo",
                ));
            }
            let (last, time, ratio) = points[points.len() - 1];
            let seconds = time + (beat - last) * ratio;
            if beat == last {
                points.pop();
            }
            points.push((beat, seconds, 60.0 / bpm));
        }
        Ok(Self(points))
    }

    fn time(&self, beat: f64) -> f64 {
        let index = self
            .0
            .partition_point(|&(start, _, _)| start <= beat)
            .saturating_sub(1);
        let (start, time, ratio) = self.0[index];
        time + (beat - start) * ratio
    }
}

struct Syllable {
    lyric: Lyric,
    notes: Vec<Note>,
}

fn syllables(notes: Vec<Note>) -> Result<Vec<Syllable>> {
    let mut syllables: Vec<Syllable> = Vec::new();
    let mut extending = false;
    let mut previous_end = 0.0;
    for note in notes {
        if note.start < previous_end - EPSILON {
            return Err(invalid(
                "selected MusicXML vocal line has overlapping notes",
            ));
        }
        let contiguous = (previous_end - note.start).abs() < EPSILON;
        previous_end = note.end;
        let lyric = note.lyric.as_ref();
        let explicit_extender = lyric.and_then(|l| l.extend.as_deref());
        if let Some(lyric) = lyric.filter(|l| !l.text.trim().is_empty()) {
            extending = matches!(lyric.extend.as_deref(), Some("start" | "continue"));
            syllables.push(Syllable {
                lyric: lyric.clone(),
                notes: vec![note],
            });
        } else if contiguous
            && (note.tied_from_previous || extending || explicit_extender.is_some())
        {
            let stop = explicit_extender == Some("stop");
            let has_extender = explicit_extender.is_some();
            if let Some(last) = syllables
                .last_mut()
                .filter(|last| (last.notes.last().unwrap().end - note.start).abs() < EPSILON)
            {
                last.notes.push(note);
            }
            extending = !stop && (extending || has_extender);
        } else {
            extending = false;
        }
    }
    Ok(syllables)
}

fn add_syllable(
    plan: &mut Plan,
    syllable: &Syllable,
    sounds: &[Sound],
    clock: &Clock,
) -> Result<()> {
    let first = &syllable.notes[0];
    let last = syllable.notes.last().unwrap();
    let start = clock.time(first.start);
    let end = clock.time(last.end);
    if !end.is_finite() || end - start < 1e-6 {
        return Err(invalid(
            "MusicXML vocal note duration is too short to render",
        ));
    }
    let first_length = clock.time(first.end) - start;
    let last_length = end - clock.time(last.start);
    let first_nucleus = sounds.iter().position(|sound| sound.is_nucleus());
    let last_nucleus = sounds.iter().rposition(|sound| sound.is_nucleus());
    if let (Some(first), Some(last)) = (first_nucleus, last_nucleus) {
        let onset = &sounds[..first];
        let coda = &sounds[last + 1..];
        let lead = onset
            .iter()
            .copied()
            .map(weight)
            .sum::<f64>()
            .min(first_length * 0.32)
            .min(0.15);
        let tail = coda
            .iter()
            .copied()
            .map(weight)
            .sum::<f64>()
            .min(last_length * 0.30)
            .min(0.14);
        distribute(plan, onset, start, start + lead);
        distribute(plan, &sounds[first..=last], start + lead, end - tail);
        distribute(plan, coda, end - tail, end);
    } else {
        distribute(plan, sounds, start, end);
    }
    for (index, note) in syllable.notes.iter().enumerate() {
        let time = clock.time(note.start);
        let note_end = clock.time(note.end);
        let hz = midi_hz(note.midi);
        // Hold each pitch. A short portamento precedes a change inside a melisma.
        if index > 0 {
            let previous = &syllable.notes[index - 1];
            if previous.midi != note.midi {
                let lead = (time - clock.time(previous.start)) * 0.15;
                let glide = time - lead.min(0.012);
                if plan.pitch.last().is_some_and(|target| target.time > glide) {
                    plan.pitch.pop();
                }
                add_pitch(plan, glide, midi_hz(previous.midi));
            }
        }
        add_pitch(plan, time, hz);
        add_pitch(plan, note_end, hz);
    }
    plan.cues.push(Cue {
        text: syllable.lyric.text.clone(),
        start,
        end,
    });
    Ok(())
}

fn add_pitch(plan: &mut Plan, time: f64, hz: f64) {
    let previous = plan
        .pitch
        .len()
        .checked_sub(2)
        .map(|index| plan.pitch[index].time);
    if let Some(last) = plan.pitch.last_mut()
        && (last.time - time).abs() < 1e-9
    {
        if last.hz == hz {
            return;
        }
        // Preserve the pitch through the end of a preceding syllable.
        if let Some(previous) = previous {
            last.time = (time - 0.0001).max((previous + time) * 0.5);
        } else if time > 0.0 {
            last.time = (time - 0.0001).max(0.0);
        } else {
            last.hz = hz;
            return;
        }
    }
    plan.pitch.push(Pitch { time, hz });
}

fn schedule(
    plan: &mut Plan,
    line: &[Syllable],
    clock: &Clock,
    pronunciations: &Pronunciations,
) -> Result<()> {
    let mut groups = line.iter();
    while let Some(first) = groups.next() {
        let mut group = vec![first];
        match first.lyric.syllabic.as_str() {
            "single" => {}
            "begin" => loop {
                let next = groups
                    .next()
                    .ok_or_else(|| invalid("unfinished MusicXML lyric word"))?;
                group.push(next);
                match next.lyric.syllabic.as_str() {
                    "middle" => {}
                    "end" => break,
                    _ => return Err(invalid("expected middle or end of MusicXML lyric word")),
                }
            },
            _ => return Err(invalid("MusicXML lyric starts in the middle of a word")),
        }
        let spellings: Vec<_> = group.iter().map(|s| s.lyric.text.trim()).collect();
        let word = if group.len() == 1 {
            spellings[0].to_owned()
        } else {
            spellings.concat()
        };
        let phonemes = lyric_syllables(&word, &spellings, pronunciations)?;
        for (syllable, sounds) in group.into_iter().zip(phonemes) {
            if sounds.is_empty() {
                return Err(invalid(format!("empty pronunciation for lyric {word:?}")));
            }
            add_syllable(plan, syllable, &sounds, clock)?;
        }
    }
    Ok(())
}

fn score_document(xml: &str) -> Result<Document<'_>> {
    // MusicXML exports may declare a DOCTYPE. roxmltree never fetches external entities.
    let document = Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000_000,
        },
    )?;
    if !named(document.root_element(), "score-partwise") {
        return Err(invalid("expected a score-partwise MusicXML document"));
    }
    Ok(document)
}

/// Read lyrics and performance timing from a MusicXML or compressed MXL score.
pub fn plan_song(
    path: impl AsRef<Path>,
    options: &SongOptions,
    pronunciations: &Pronunciations,
) -> Result<Plan> {
    let xml = score_xml(path.as_ref())?;
    let document = score_document(&xml)?;
    let root = document.root_element();
    let LyricLine {
        part,
        voice,
        staff,
        verse,
        ..
    } = select(root, options)?;
    let conductor = root.children().find(|&node| named(node, "part")).unwrap();
    let line = parse_line(part, Some((voice, staff, verse)))?;
    let mut tempos = if conductor != part {
        parse_line(conductor, None)?.tempos
    } else {
        Vec::new()
    };
    tempos.extend(line.tempos);
    let default = options.default_bpm.unwrap_or(120.0);
    if !default.is_finite() || default <= 0.0 {
        return Err(invalid("default tempo must be positive and finite"));
    }
    let clock = Clock::new(tempos, default)?;
    let syllables = syllables(line.notes)?;
    let mut plan = Plan {
        duration: clock.time(line.beats),
        ..Plan::default()
    };
    schedule(&mut plan, &syllables, &clock, pronunciations)?;
    Ok(plan)
}
