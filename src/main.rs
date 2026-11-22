mod web;

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use jtalk::{
    Pronunciations, Result, SongOptions, Voice, plan_song, plan_speech, render, write_wav,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    output: Output,
}

#[derive(Subcommand)]
enum Command {
    /// Serve a local browser interface for text to speech.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
    },
    /// Speak English text.
    Speak {
        text: String,
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
    },
    /// Sing embedded lyrics from a MusicXML or MXL score.
    Sing {
        score: PathBuf,
        #[arg(long)]
        part: Option<String>,
        #[arg(long)]
        verse: Option<String>,
        #[arg(long)]
        voice: Option<String>,
        #[arg(long)]
        staff: Option<String>,
        #[arg(long)]
        bpm: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Preset {
    Default,
    Robot,
}

#[derive(Args)]
struct Output {
    /// WAV output path, or "-" for standard output.
    #[arg(short, long, global = true, default_value = "-")]
    output: PathBuf,
    #[arg(long, global = true, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, global = true, value_enum)]
    preset: Option<Preset>,
    /// Speaking pitch in Hz. Singing takes pitch from the score.
    #[arg(long, global = true)]
    pitch: Option<f64>,
    #[arg(long, global = true)]
    formant_scale: Option<f64>,
    #[arg(long, global = true)]
    breath: Option<f64>,
    /// Period-to-period pitch variation, in cents.
    #[arg(long, global = true)]
    jitter: Option<f64>,
    /// Vibrato depth in cents.
    #[arg(long, global = true)]
    vibrato: Option<f64>,
    #[arg(long, global = true)]
    vibrato_hz: Option<f64>,
    #[arg(long, global = true)]
    gain: Option<f64>,
    /// Text file with one "word PHONE PHONE ..." override per line.
    #[arg(long, global = true)]
    pronunciations: Option<PathBuf>,
}

fn load_overrides(path: Option<&Path>) -> Result<Pronunciations> {
    let mut pronunciations = Pronunciations::default();
    if let Some(path) = path {
        let reader = BufReader::new(File::open(path)?);
        for (index, line) in reader.lines().enumerate() {
            let line = line?;
            let rule = line.split('#').next().unwrap().trim();
            if rule.is_empty() {
                continue;
            }
            let (word, phones) = rule.split_once(char::is_whitespace).unwrap_or((rule, ""));
            pronunciations.insert(word, phones).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}: {error}", path.display(), index + 1),
                )
            })?;
        }
    }
    Ok(pronunciations)
}

fn run(cli: Cli) -> Result<()> {
    let settings = cli.output;
    let preset = match settings.preset.unwrap_or(Preset::Default) {
        Preset::Default => Voice::default(),
        Preset::Robot => Voice::robot(),
    };
    let voice = Voice {
        speech_hz: settings.pitch.unwrap_or(preset.speech_hz),
        formant_scale: settings.formant_scale.unwrap_or(preset.formant_scale),
        breath: settings.breath.unwrap_or(preset.breath),
        jitter_cents: settings.jitter.unwrap_or(preset.jitter_cents),
        vibrato_cents: settings.vibrato.unwrap_or(preset.vibrato_cents),
        vibrato_hz: settings.vibrato_hz.unwrap_or(preset.vibrato_hz),
        gain: settings.gain.unwrap_or(preset.gain),
    };
    let pronunciations = load_overrides(settings.pronunciations.as_deref())?;
    let plan = match cli.command {
        Command::Serve { bind } => return web::serve(bind, &pronunciations),
        Command::Speak { text, speed } => plan_speech(&text, voice, speed, &pronunciations)?,
        Command::Sing {
            score,
            part,
            verse,
            voice,
            staff,
            bpm,
        } => {
            let options = SongOptions {
                part,
                verse,
                voice,
                staff,
                default_bpm: bpm,
            };
            plan_song(score, &options, &pronunciations)?
        }
    };
    let audio = render(&plan, voice, settings.sample_rate)?;
    if settings.output == Path::new("-") {
        write_wav(&audio, BufWriter::new(io::stdout().lock()))
    } else {
        write_wav(&audio, BufWriter::new(File::create(settings.output)?))
    }
}

fn main() -> std::process::ExitCode {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("jtalk: {error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
