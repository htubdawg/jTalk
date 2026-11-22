use std::net::SocketAddr;

use jtalk::{Pronunciations, Result, Voice, plan_speech, render, write_wav};
use serde::Deserialize;
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_BODY: u64 = 8192;
const MAX_TEXT_CHARS: usize = 600;
const SAMPLE_RATE: u32 = 48_000;

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Preset {
    #[default]
    Default,
    Robot,
}

#[derive(Deserialize)]
struct SpeechRequest {
    text: String,
    #[serde(default)]
    preset: Preset,
    pitch: Option<f64>,
    speed: Option<f64>,
    formant_scale: Option<f64>,
    breath: Option<f64>,
}

pub fn serve(bind: SocketAddr, pronunciations: &Pronunciations) -> Result<()> {
    let server = Server::http(bind)?;
    eprintln!("jTalk listening at http://{}", server.server_addr());
    for mut request in server.incoming_requests() {
        let response = match (request.method(), request.url()) {
            (&Method::Get, "/") => asset(
                "text/html; charset=utf-8",
                include_bytes!("../web/index.html"),
            ),
            (&Method::Get, "/style.css") => asset(
                "text/css; charset=utf-8",
                include_bytes!("../web/style.css"),
            ),
            (&Method::Get, "/app.js") => asset(
                "text/javascript; charset=utf-8",
                include_bytes!("../web/app.js"),
            ),
            (&Method::Post, "/api/speak") => speech_response(&mut request, pronunciations),
            _ => message(404, "Page not found."),
        };
        if let Err(error) = request.respond(response)
            && !matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            )
        {
            eprintln!("jTalk could not send a response ({error})");
        }
    }
    Ok(())
}

type WebResponse = Response<std::io::Cursor<Vec<u8>>>;

fn response(status: u16, kind: &str, body: Vec<u8>) -> WebResponse {
    Response::from_data(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", kind).expect("valid content type"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").expect("valid cache header"))
}

fn asset(kind: &str, body: &[u8]) -> WebResponse {
    response(200, kind, body.to_vec())
}

fn message(status: u16, text: &str) -> WebResponse {
    response(
        status,
        "text/plain; charset=utf-8",
        text.as_bytes().to_vec(),
    )
}

fn speech_response(request: &mut Request, pronunciations: &Pronunciations) -> WebResponse {
    let is_json = request.headers().iter().any(|header| {
        header.field.equiv("Content-Type")
            && header
                .value
                .as_str()
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
    });
    if !is_json {
        return message(415, "Send the speech request as JSON.");
    }
    if request
        .body_length()
        .is_some_and(|length| length as u64 > MAX_BODY)
    {
        return message(413, "The speech request is too large.");
    }
    use std::io::Read;
    let mut body = Vec::new();
    if request
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .is_err()
    {
        return message(400, "Could not read the speech request.");
    }
    if body.len() as u64 > MAX_BODY {
        return message(413, "The speech request is too large.");
    }
    let Ok(input) = serde_json::from_slice::<SpeechRequest>(&body) else {
        return message(400, "The speech request contains invalid JSON or settings.");
    };
    if input.text.chars().count() > MAX_TEXT_CHARS {
        return message(422, "Enter 600 characters or fewer.");
    }

    let mut voice = match input.preset {
        Preset::Default => Voice::default(),
        Preset::Robot => Voice::robot(),
    };
    voice.speech_hz = input.pitch.unwrap_or(voice.speech_hz);
    voice.formant_scale = input.formant_scale.unwrap_or(voice.formant_scale);
    voice.breath = input.breath.unwrap_or(voice.breath);
    let speed = input.speed.unwrap_or(1.0);
    let plan = match plan_speech(&input.text, voice, speed, pronunciations) {
        Ok(plan) => plan,
        Err(error) => return message(422, &error.to_string()),
    };
    if plan.duration > 45.0 {
        return message(
            422,
            "This line is longer than 45 seconds. Shorten it or increase the speed.",
        );
    }
    let audio = match render(&plan, voice, SAMPLE_RATE) {
        Ok(audio) => audio,
        Err(error) => return message(422, &error.to_string()),
    };
    let mut wav = Vec::new();
    match write_wav(&audio, &mut wav) {
        Ok(()) => response(200, "audio/wav", wav),
        Err(_) => message(500, "Could not write the WAV."),
    }
}
