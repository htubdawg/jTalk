use std::cell::RefCell;
use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

use jtalk::{Pronunciations, Voice, plan_speech, render, write_wav};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, CreateFontW, DeleteObject, GetSysColorBrush, HFONT,
};
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetSaveFileNameW, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{
    EM_SETLIMITTEXT, EM_SETSEL, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN, TBS_NOTICKS, TRACKBAR_CLASSW,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetKeyState, SetFocus, VK_CONTROL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::w;

const PRESET: usize = 101;
const RENDER: usize = 102;
const REPLAY: usize = 103;
const STOP: usize = 104;
const SAVE: usize = 105;
const RENDER_TIMER: usize = 1;
const TBM_GETPOS: u32 = WM_USER;
const SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BREATH: i32 = 35;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn set_text(window: HWND, text: &str) {
    unsafe { SetWindowTextW(window, wide(text).as_ptr()) };
}

fn scaled(value: i32, dpi: u32) -> i32 {
    (value * dpi as i32 + 48) / 96
}

#[derive(Clone, Copy, Default)]
struct Slider {
    track: HWND,
    value: HWND,
}

impl Slider {
    fn position(self) -> i32 {
        unsafe { SendMessageW(self.track, TBM_GETPOS, 0, 0) as i32 }
    }

    fn set(self, position: i32) {
        unsafe { SendMessageW(self.track, TBM_SETPOS, 1, position as isize) };
    }
}

type Rendered = Result<Vec<u8>, String>;

#[derive(Default)]
struct App {
    instance: HINSTANCE,
    dpi: u32,
    body_font: HFONT,
    heading_font: HFONT,
    text: HWND,
    preset: HWND,
    pitch: Slider,
    speed: Slider,
    formant: Slider,
    breath: Slider,
    render_button: HWND,
    replay_button: HWND,
    stop_button: HWND,
    save_button: HWND,
    status: HWND,
    task: Option<Receiver<Rendered>>,
    wav: Vec<u8>,
}

impl App {
    fn control(
        &self,
        parent: HWND,
        class: *const u16,
        text: &str,
        style: u32,
        id: usize,
        bounds: [i32; 4],
    ) -> HWND {
        let [x, y, width, height] = bounds.map(|value| scaled(value, self.dpi));
        // Win32 copies the title during creation. A child window's HMENU is its control ID.
        unsafe {
            let child = CreateWindowExW(
                0,
                class,
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                x,
                y,
                width,
                height,
                parent,
                id as HMENU,
                self.instance,
                null(),
            );
            SendMessageW(child, WM_SETFONT, self.body_font as usize, 1);
            child
        }
    }

    fn label(&self, parent: HWND, text: &str, bounds: [i32; 4]) -> HWND {
        self.control(parent, w!("STATIC"), text, 0, 0, bounds)
    }

    fn slider(&self, parent: HWND, label: &str, x: i32, y: i32, range: [i32; 3]) -> Slider {
        let [minimum, maximum, initial] = range;
        self.label(parent, label, [x, y, 150, 20]);
        let value = self.label(parent, "", [x + 165, y, 105, 20]);
        let track = self.control(
            parent,
            TRACKBAR_CLASSW,
            "",
            WS_TABSTOP | TBS_NOTICKS,
            0,
            [x, y + 20, 270, 28],
        );
        unsafe {
            SendMessageW(track, TBM_SETRANGEMIN, 0, minimum as isize);
            SendMessageW(track, TBM_SETRANGEMAX, 0, maximum as isize);
        }
        let slider = Slider { track, value };
        slider.set(initial);
        slider
    }

    fn build(&mut self, window: HWND) {
        self.body_font = font(self.dpi, 15, 400);
        self.heading_font = font(self.dpi, 28, 650);
        let heading = self.label(window, "Text to speech", [24, 19, 586, 37]);
        unsafe { SendMessageW(heading, WM_SETFONT, self.heading_font as usize, 1) };
        self.label(window, "Dialogue", [24, 65, 572, 21]);
        self.text = self.control(
            window,
            w!("EDIT"),
            "Stay with me. Can you hear me?",
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32,
            0,
            [24, 90, 572, 129],
        );
        unsafe { SendMessageW(self.text, EM_SETLIMITTEXT, 600, 0) };
        self.label(window, "Voice preset", [24, 238, 115, 23]);
        self.preset = self.control(
            window,
            w!("COMBOBOX"),
            "",
            WS_TABSTOP | CBS_DROPDOWNLIST as u32,
            PRESET,
            [138, 232, 180, 140],
        );
        unsafe {
            SendMessageW(self.preset, CB_ADDSTRING, 0, w!("Default") as isize);
            SendMessageW(self.preset, CB_ADDSTRING, 0, w!("Robot") as isize);
            SendMessageW(self.preset, CB_SETCURSEL, 0, 0);
        }
        self.pitch = self.slider(window, "Pitch", 24, 279, [65, 320, 135]);
        self.speed = self.slider(window, "Speed", 326, 279, [50, 200, 100]);
        self.formant = self.slider(window, "Formant scale", 24, 338, [65, 145, 100]);
        self.breath = self.slider(window, "Breath", 326, 338, [0, 100, DEFAULT_BREATH]);
        self.render_button = self.control(
            window,
            w!("BUTTON"),
            "Render && Play",
            WS_TABSTOP,
            RENDER,
            [24, 405, 151, 35],
        );
        self.replay_button = self.control(
            window,
            w!("BUTTON"),
            "Play again",
            WS_TABSTOP,
            REPLAY,
            [186, 405, 108, 35],
        );
        self.stop_button = self.control(
            window,
            w!("BUTTON"),
            "Stop",
            WS_TABSTOP,
            STOP,
            [305, 405, 83, 35],
        );
        self.save_button = self.control(
            window,
            w!("BUTTON"),
            "Save WAV…",
            WS_TABSTOP,
            SAVE,
            [399, 405, 113, 35],
        );
        self.status = self.label(window, "", [24, 454, 572, 55]);
        self.update_sliders();
        self.update_buttons();
        unsafe { SetFocus(self.text) };
    }

    fn update_sliders(&self) {
        set_text(self.pitch.value, &format!("{} Hz", self.pitch.position()));
        set_text(
            self.speed.value,
            &format!("{:.2}×", f64::from(self.speed.position()) / 100.0),
        );
        set_text(
            self.formant.value,
            &format!("{:.2}×", f64::from(self.formant.position()) / 100.0),
        );
        set_text(self.breath.value, &format!("{}%", self.breath.position()));
    }

    fn update_buttons(&self) {
        let idle = self.task.is_none();
        let has_audio = idle && !self.wav.is_empty();
        unsafe {
            EnableWindow(self.render_button, idle.into());
            EnableWindow(self.replay_button, has_audio.into());
            EnableWindow(self.stop_button, has_audio.into());
            EnableWindow(self.save_button, has_audio.into());
        }
    }

    fn is_robot(&self) -> bool {
        unsafe { SendMessageW(self.preset, CB_GETCURSEL, 0, 0) == 1 }
    }

    fn render(&mut self, window: HWND) {
        if self.task.is_some() {
            return;
        }
        let mut buffer = [0u16; 601];
        let length = unsafe { GetWindowTextW(self.text, buffer.as_mut_ptr(), buffer.len() as i32) };
        let text = String::from_utf16_lossy(&buffer[..length as usize]);
        if text.trim().is_empty() {
            set_text(self.status, "Enter a line of dialogue.");
            unsafe { SetFocus(self.text) };
            return;
        }
        let preset = if self.is_robot() {
            Voice::robot()
        } else {
            Voice::default()
        };
        let voice = Voice {
            speech_hz: f64::from(self.pitch.position()),
            formant_scale: f64::from(self.formant.position()) / 100.0,
            breath: f64::from(self.breath.position()) / 100.0,
            ..preset
        };
        let speed = f64::from(self.speed.position()) / 100.0;
        if unsafe { SetTimer(window, RENDER_TIMER, 40, None) } == 0 {
            set_text(self.status, "Could not start the renderer.");
            return;
        }
        stop_audio();
        let (sender, receiver) = channel();
        match thread::Builder::new().spawn(move || {
            let _ = sender.send(render_wav(&text, voice, speed).map_err(|error| error.to_string()));
        }) {
            Ok(_) => {
                self.task = Some(receiver);
                self.update_buttons();
                set_text(self.status, "Rendering…");
            }
            Err(error) => {
                unsafe { KillTimer(window, RENDER_TIMER) };
                set_text(
                    self.status,
                    &format!("Could not start the renderer ({error})."),
                );
            }
        }
    }

    fn render_ready(&mut self, window: HWND) {
        let Some(task) = &self.task else { return };
        let result = match task.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The renderer stopped unexpectedly.".into()),
        };
        unsafe { KillTimer(window, RENDER_TIMER) };
        self.task = None;
        match result {
            Ok(wav) => {
                self.wav = wav;
                self.play();
            }
            Err(error) => set_text(self.status, &error),
        }
        self.update_buttons();
    }

    fn play(&self) {
        if self.wav.is_empty() {
            return;
        }
        // PlaySound borrows the buffer during asynchronous playback. It stays
        // in App until a new render explicitly stops playback or the window closes.
        let played = unsafe {
            PlaySoundW(
                self.wav.as_ptr().cast(),
                null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            )
        };
        if played != 0 {
            set_text(self.status, "Audio ready.");
        } else {
            set_text(
                self.status,
                "Could not play audio. You can still save the WAV.",
            );
        }
    }

    fn save(&self, window: HWND) {
        if self.wav.is_empty() {
            return;
        }
        match save_path(window) {
            Ok(Some(path)) => match std::fs::write(&path, &self.wav) {
                Ok(()) => set_text(self.status, &format!("Saved {}.", path.display())),
                Err(error) => set_text(self.status, &format!("Could not save the WAV ({error}).")),
            },
            Ok(None) => {}
            Err(error) => set_text(self.status, &error),
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        stop_audio();
        unsafe {
            DeleteObject(self.body_font);
            DeleteObject(self.heading_font);
        }
    }
}

fn render_wav(text: &str, voice: Voice, speed: f64) -> jtalk::Result<Vec<u8>> {
    let plan = plan_speech(text, voice, speed, &Pronunciations::default())?;
    let audio = render(&plan, voice, SAMPLE_RATE)?;
    let mut wav = Vec::new();
    write_wav(&audio, &mut wav)?;
    Ok(wav)
}

fn stop_audio() {
    unsafe { PlaySoundW(null(), null_mut(), 0) };
}

fn font(dpi: u32, height: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -scaled(height, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            w!("Segoe UI"),
        )
    }
}

fn save_path(owner: HWND) -> Result<Option<PathBuf>, String> {
    let mut path = [0u16; 32_768];
    let name = wide("jtalk-dialogue.wav");
    path[..name.len()].copy_from_slice(&name);
    let filter = wide("WAV audio\0*.wav\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: path.as_mut_ptr(),
        nMaxFile: path.len() as u32,
        lpstrDefExt: w!("wav"),
        Flags: OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetSaveFileNameW(&mut dialog) } != 0 {
        let end = path
            .iter()
            .position(|&code| code == 0)
            .unwrap_or(path.len());
        return Ok(Some(PathBuf::from(OsString::from_wide(&path[..end]))));
    }
    let error = unsafe { CommDlgExtendedError() };
    if error == 0 {
        Ok(None)
    } else {
        Err(format!("Could not open the save dialog ({error:#x})."))
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        if message == WM_DESTROY {
            stop_audio();
            PostQuitMessage(0);
            return 0;
        }

        // run() owns this state for the life of the message loop. Use a checked
        // borrow because Windows may deliver a message during a control call.
        let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<App>;
        if let Some(state) = state.as_ref()
            && let Ok(mut app) = state.try_borrow_mut()
        {
            match message {
                WM_CREATE => app.build(window),
                WM_HSCROLL => app.update_sliders(),
                WM_TIMER if wparam == RENDER_TIMER => app.render_ready(window),
                WM_COMMAND => {
                    let id = wparam & 0xffff;
                    let notification = ((wparam >> 16) & 0xffff) as u32;
                    match id {
                        PRESET if notification == CBN_SELCHANGE => {
                            app.breath
                                .set(if app.is_robot() { 0 } else { DEFAULT_BREATH });
                            app.update_sliders();
                        }
                        RENDER if notification == BN_CLICKED => app.render(window),
                        REPLAY if notification == BN_CLICKED && app.task.is_none() => app.play(),
                        STOP if notification == BN_CLICKED => {
                            stop_audio();
                            set_text(app.status, "Playback stopped.");
                        }
                        SAVE if notification == BN_CLICKED && app.task.is_none() => {
                            app.save(window)
                        }
                        _ => return DefWindowProcW(window, message, wparam, lparam),
                    }
                }
                _ => return DefWindowProcW(window, message, wparam, lparam),
            }
            return 0;
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

pub fn run() {
    if let Err(error) = run_window() {
        unsafe {
            MessageBoxW(
                null_mut(),
                wide(&error.to_string()).as_ptr(),
                w!("jTalk"),
                MB_OK | MB_ICONERROR,
            )
        };
    }
}

fn run_window() -> std::io::Result<()> {
    unsafe {
        SetProcessDPIAware();
        let dpi = GetDpiForSystem().max(96);
        let instance = GetModuleHandleW(null());
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&controls);

        let class = w!("jTalkDesktopWindow");
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&descriptor) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut bounds = RECT {
            right: scaled(620, dpi),
            bottom: scaled(517, dpi),
            ..Default::default()
        };
        AdjustWindowRectEx(&mut bounds, style, 0, 0);
        let mut state = RefCell::new(App::default());
        state.get_mut().instance = instance;
        state.get_mut().dpi = dpi;
        let window = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("jTalk"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            null_mut(),
            null_mut(),
            instance,
            (&state as *const RefCell<App>).cast(),
        );
        if window.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        ShowWindow(window, SW_SHOWNORMAL);
        let editor = state.borrow().text;
        SetFocus(editor);
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, null_mut(), 0, 0) {
                0 => return Ok(()),
                -1 => {
                    let error = std::io::Error::last_os_error();
                    DestroyWindow(window);
                    return Err(error);
                }
                _ => {
                    if message.hwnd == editor
                        && message.message == WM_KEYDOWN
                        && message.wParam == usize::from(b'A')
                        && GetKeyState(i32::from(VK_CONTROL)) < 0
                    {
                        SendMessageW(editor, EM_SETSEL, 0, -1);
                        continue;
                    }
                    if IsDialogMessageW(window, &message) == 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }
}
