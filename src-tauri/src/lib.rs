pub mod audio;
pub mod config;
pub mod model_manager;
pub mod tts;

use audio::AudioPlayer;
use config::{AppSettings, load_settings, save_settings, model_dir};
use model_manager::is_model_ready;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use tauri::{
    AppHandle, Emitter, Manager, State,
    ipc::Channel,
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_clipboard_manager::ClipboardExt;
use opener::open as open_path;

pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub tts_engine: Mutex<Option<Arc<tts::TtsEngine>>>,
    pub tts_engine_error: Mutex<String>,
    pub audio_player: Arc<AudioPlayer>,
    pub download_progress: AtomicU64,
    pub last_text: Arc<Mutex<String>>,
    pub is_downloading: AtomicBool,
}

const VALID_LANGS: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr",
    "hi", "hr", "hu", "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk",
    "sl", "sv", "tr", "uk", "vi",
];

#[tauri::command]
fn save_settings_cmd(mut settings: AppSettings, state: State<AppState>, app: AppHandle) -> Result<(), String> {
    println!("[save_settings_cmd] received quality={} speed={} silence={} language='{}' voice_style='{}' shortcut='{}' pause_shortcut='{}'",
        settings.quality, settings.speed, settings.silence, settings.language, settings.voice_style, settings.shortcut, settings.pause_shortcut);
    if !VALID_LANGS.contains(&settings.language.as_str()) {
        println!("[save_settings_cmd] Invalid language '{}', falling back to 'en'", settings.language);
        settings.language = "en".to_string();
    }

    let (old_voice_style, old_shortcut, old_pause_shortcut) = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        (s.voice_style.clone(), s.shortcut.clone(), s.pause_shortcut.clone())
    };

    save_settings(&settings).map_err(|e| e.to_string())?;
    *state.settings.lock().map_err(|e| e.to_string())? = settings.clone();
    println!("[save_settings_cmd] saved ok quality={} speed={} silence={} language='{}' voice_style='{}' shortcut='{}' pause_shortcut='{}'",
        settings.quality, settings.speed, settings.silence, settings.language, settings.voice_style, settings.shortcut, settings.pause_shortcut);

    // voice style 变化时热更新引擎
    if settings.voice_style != old_voice_style {
        let engine_arc = {
            let engine_opt = state.tts_engine.lock().map_err(|e| e.to_string())?;
            engine_opt.as_ref().cloned()
        };
        if let Some(engine) = engine_arc {
            let style_path = model_dir().join("voice_styles").join(format!("{}.json", settings.voice_style));
            match tts::helper::load_voice_style(&style_path) {
                Ok(new_style) => {
                    if let Err(e) = engine.replace_style(new_style) {
                        println!("[save_settings_cmd] failed to swap voice style: {}", e);
                    } else {
                        println!("[save_settings_cmd] voice style hot-updated to '{}'", settings.voice_style);
                    }
                }
                Err(e) => {
                    println!("[save_settings_cmd] failed to load voice style '{}': {}", settings.voice_style, e);
                }
            }
        }
    }

    // 快捷键变化时重新注册
    if settings.shortcut != old_shortcut || settings.pause_shortcut != old_pause_shortcut {
        let _ = app.global_shortcut().unregister_all();
        match register_read_shortcut(&app, &settings.shortcut) {
            Ok(_) => println!("[save_settings_cmd] read shortcut registered '{}'", settings.shortcut),
            Err(e) => println!("[save_settings_cmd] failed to register read shortcut '{}': {}", settings.shortcut, e),
        }
        if !settings.pause_shortcut.trim().is_empty() {
            match register_pause_shortcut(&app, &settings.pause_shortcut) {
                Ok(_) => println!("[save_settings_cmd] pause shortcut registered '{}'", settings.pause_shortcut),
                Err(e) => println!("[save_settings_cmd] failed to register pause shortcut '{}': {}", settings.pause_shortcut, e),
            }
        }
    }

    Ok(())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    println!("[get_settings] returning quality={} speed={} silence={} language='{}' voice_style='{}' shortcut='{}'",
        settings.quality, settings.speed, settings.silence, settings.language, settings.voice_style, settings.shortcut);
    Ok(settings.clone())
}

#[tauri::command]
fn get_tts_state(state: State<AppState>) -> Result<TtsState, String> {
    let ready = state.tts_engine.lock().map_err(|e| e.to_string())?.is_some();
    let downloading = state.is_downloading.load(Ordering::Relaxed);
    let progress = state.download_progress.load(Ordering::Relaxed);
    let last_text = state.last_text.lock().map_err(|e| e.to_string())?.clone();
    let error = state.tts_engine_error.lock().map_err(|e| e.to_string())?.clone();
    Ok(TtsState { ready, downloading, progress, last_text, error })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TtsState {
    ready: bool,
    downloading: bool,
    progress: u64,
    last_text: String,
    error: String,
}

#[tauri::command]
fn open_model_directory() -> Result<(), String> {
    let dir = model_dir();
    println!("[open_model_directory] target dir: {:?}", dir);
    std::fs::create_dir_all(&dir).map_err(|e| {
        let msg = format!("创建目录失败: {}", e);
        println!("[open_model_directory] {}", msg);
        msg
    })?;
    open_path(&dir).map_err(|e| {
        let msg = format!("打开目录失败: {}", e);
        println!("[open_model_directory] {}", msg);
        msg
    })
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    open_path(&url).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("open-settings", ());
    }
    Ok(())
}

#[tauri::command]
async fn download_model_cmd(
    channel: Channel<u64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    download_model_with_base(channel, state, app, model_manager::HF_BASE_URL).await
}

#[tauri::command]
async fn download_model_mirror_cmd(
    channel: Channel<u64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    download_model_with_base(channel, state, app, model_manager::HF_MIRROR_URL).await
}

async fn download_model_with_base(
    channel: Channel<u64>,
    state: State<'_, AppState>,
    app: AppHandle,
    base_url: &str,
) -> Result<(), String> {
    if state.is_downloading.load(Ordering::Relaxed) {
        return Err("下载正在进行中，请等待".to_string());
    }
    state.is_downloading.store(true, Ordering::Relaxed);
    state.download_progress.store(0, Ordering::Relaxed);

    let dir = model_dir();
    let result = model_manager::download_model(&dir, base_url, move |progress, _total| {
        let _ = channel.send(progress);
    }).await.map_err(|e| e.to_string());

    state.is_downloading.store(false, Ordering::Relaxed);

    if result.is_ok() {
        let voice_style = state.settings.lock().map_err(|e| e.to_string())?.voice_style.clone();
        match tts::TtsEngine::new(&dir, &voice_style) {
            Ok(engine) => {
                *state.tts_engine.lock().map_err(|e| e.to_string())? = Some(Arc::new(engine));
                *state.tts_engine_error.lock().map_err(|e| e.to_string())? = String::new();
                let _ = app.emit("model-ready", ());
            }
            Err(e) => {
                let err = format!("TtsEngine load failed after download: {}", e);
                println!("[download] {}", err);
                *state.tts_engine_error.lock().map_err(|e| e.to_string())? = err;
            }
        }
    }

    result
}

async fn run_read_from_clipboard(
    app: AppHandle,
    engine: Arc<tts::TtsEngine>,
    player: Arc<AudioPlayer>,
    last_text_slot: Arc<Mutex<String>>,
    language: String,
    steps: usize,
    speed: f32,
    silence: f32,
) -> Result<(), String> {
    simulate_copy();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let text: String = app.clipboard().read_text().map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Err("剪贴板为空，请先选中文字".to_string());
    }

    // Push the captured text to the UI immediately so the user sees it
    // before synthesis finishes (long inputs can take several seconds).
    if let Ok(mut slot) = last_text_slot.lock() {
        *slot = text.clone();
    }
    let _ = app.emit("tts-text-captured", text.clone());

    println!("[read_selected_text] lang='{}' text='{}' steps={} speed={} silence={}",
        language, text, steps, speed, silence);

    let sample_rate = engine.sample_rate as u32;
    let silence_len = (silence * engine.sample_rate as f32) as usize;
    let text_for_synth = text.clone();

    let result = tokio::task::spawn_blocking(move || {
        engine.synthesize_streaming(
            &text_for_synth,
            &language,
            steps,
            speed,
            silence,
            |chunk, _dur, idx| {
                if idx == 0 {
                    player.play_wav(chunk.to_vec(), sample_rate)
                        .map_err(|e| anyhow::anyhow!("play_wav: {}", e))?;
                } else {
                    if silence_len > 0 {
                        player.append_wav(vec![0.0f32; silence_len], sample_rate)
                            .map_err(|e| anyhow::anyhow!("append silence: {}", e))?;
                    }
                    player.append_wav(chunk.to_vec(), sample_rate)
                        .map_err(|e| anyhow::anyhow!("append_wav: {}", e))?;
                }
                Ok(())
            },
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {}", e))?;
    result.map_err(|e| e.to_string())?;

    let _ = app.emit("tts-played", ());

    Ok(())
}

#[tauri::command]
async fn read_selected_text(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let (language, steps, speed, silence) = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        (
            settings.language.clone(),
            clamp_quality(settings.quality) as usize,
            clamp_speed(settings.speed),
            clamp_silence(settings.silence),
        )
    };
    let engine = {
        let engine_opt = state.tts_engine.lock().map_err(|e| e.to_string())?;
        match engine_opt.as_ref() {
            Some(e) => Arc::clone(e),
            None => return Err("TTS engine not ready".to_string()),
        }
    };
    let player = Arc::clone(&state.audio_player);
    let last_text = Arc::clone(&state.last_text);
    run_read_from_clipboard(app, engine, player, last_text, language, steps, speed, silence).await
}

#[tauri::command]
fn stop_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_player.stop().map_err(|e| e.to_string())
}

#[tauri::command]
fn pause_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_player.pause().map_err(|e| e.to_string())
}

#[tauri::command]
fn resume_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_player.resume().map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_tts_cmd(
    text: String,
    language: String,
    quality: i32,
    speed: f32,
    silence: f32,
    voice_style: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let engine = {
        let engine_opt = state.tts_engine.lock().map_err(|e| e.to_string())?;
        match engine_opt.as_ref() {
            Some(e) => Arc::clone(e),
            None => return Err("TTS engine not ready".to_string()),
        }
    };

    let steps = clamp_quality(quality) as usize;
    let speed_clamped = clamp_speed(speed);
    let silence_clamped = clamp_silence(silence);
    println!("[test_tts_cmd] lang='{}' style='{}' steps={} speed={} silence={} text='{}'",
        language, voice_style, steps, speed_clamped, silence_clamped, text);

    let style_path = model_dir().join("voice_styles").join(format!("{}.json", voice_style));
    let style = tts::helper::load_voice_style(&style_path).map_err(|e| e.to_string())?;

    let sample_rate = engine.sample_rate as u32;
    let silence_len = (silence_clamped * engine.sample_rate as f32) as usize;
    let player = Arc::clone(&state.audio_player);

    let result = tokio::task::spawn_blocking(move || {
        engine.synthesize_streaming_with_style(
            &text,
            &language,
            steps,
            speed_clamped,
            silence_clamped,
            &style,
            |chunk, _dur, idx| {
                if idx == 0 {
                    player.play_wav(chunk.to_vec(), sample_rate)
                        .map_err(|e| anyhow::anyhow!("play_wav: {}", e))?;
                } else {
                    if silence_len > 0 {
                        player.append_wav(vec![0.0f32; silence_len], sample_rate)
                            .map_err(|e| anyhow::anyhow!("append silence: {}", e))?;
                    }
                    player.append_wav(chunk.to_vec(), sample_rate)
                        .map_err(|e| anyhow::anyhow!("append_wav: {}", e))?;
                }
                Ok(())
            },
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {}", e))?;
    result.map_err(|e| e.to_string())?;

    Ok(())
}

fn clamp_quality(q: i32) -> i32 {
    q.clamp(2, 16)
}

fn clamp_speed(s: f32) -> f32 {
    (s.clamp(0.8, 1.3) * 100.0).round() / 100.0
}

fn clamp_silence(s: f32) -> f32 {
    (s.clamp(0.3, 1.0) * 100.0).round() / 100.0
}

#[cfg(target_os = "windows")]
fn simulate_copy() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, VK_CONTROL, VK_C, KEYEVENTF_KEYUP,
    };
    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_C.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_C.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(not(target_os = "windows"))]
fn simulate_copy() {}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(init_state())
        .invoke_handler(tauri::generate_handler![
            save_settings_cmd,
            get_settings,
            get_tts_state,
            open_model_directory,
            open_url,
            open_settings_window,
            download_model_cmd,
            download_model_mirror_cmd,
            read_selected_text,
            stop_playback,
            pause_playback,
            resume_playback,
            test_tts_cmd,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            setup_tray(&app_handle)?;
            setup_shortcuts(&app_handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_state() -> AppState {
    let settings = load_settings().unwrap_or_default();
    let model_dir = model_dir();
    let (tts_engine, tts_engine_error) = if is_model_ready(&model_dir) {
        match tts::TtsEngine::new(&model_dir, &settings.voice_style) {
            Ok(engine) => {
                println!("[init] TtsEngine loaded successfully with voice_style='{}'", settings.voice_style);
                (Some(Arc::new(engine)), String::new())
            }
            Err(e) => {
                let err = format!("TtsEngine load failed: {}", e);
                println!("[init] {}", err);
                (None, err)
            }
        }
    } else {
        println!("[init] Model not ready, files missing");
        (None, String::from("Model files missing"))
    };

    AppState {
        settings: Mutex::new(settings),
        tts_engine: Mutex::new(tts_engine),
        tts_engine_error: Mutex::new(tts_engine_error),
        audio_player: Arc::new(AudioPlayer::new().expect("Failed to create audio player")),
        download_progress: AtomicU64::new(0),
        last_text: Arc::new(Mutex::new(String::new())),
        is_downloading: AtomicBool::new(false),
    }
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "打开主面板", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &settings_i, &quit_i])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
            "settings" => {
                let _ = open_settings_window(app.clone());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn parse_shortcut_str(s: &str) -> Result<Shortcut, String> {
    use tauri_plugin_global_shortcut::{Code, Modifiers};
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let key_part = parts.last().unwrap();
    let mod_parts = &parts[..parts.len().saturating_sub(1)];

    let mut mods = Modifiers::empty();
    for m in mod_parts {
        match *m {
            "Ctrl" => mods |= Modifiers::CONTROL,
            "Alt" => mods |= Modifiers::ALT,
            "Shift" => mods |= Modifiers::SHIFT,
            "Super" | "Win" | "Cmd" => mods |= Modifiers::SUPER,
            _ => return Err(format!("未知修饰键: {}", m)),
        }
    }

    let code = match *key_part {
        "`" | "Backquote" => Code::Backquote,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "0" => Code::Digit0,
        "A" | "a" => Code::KeyA,
        "B" | "b" => Code::KeyB,
        "C" | "c" => Code::KeyC,
        "D" | "d" => Code::KeyD,
        "E" | "e" => Code::KeyE,
        "F" | "f" => Code::KeyF,
        "G" | "g" => Code::KeyG,
        "H" | "h" => Code::KeyH,
        "I" | "i" => Code::KeyI,
        "J" | "j" => Code::KeyJ,
        "K" | "k" => Code::KeyK,
        "L" | "l" => Code::KeyL,
        "M" | "m" => Code::KeyM,
        "N" | "n" => Code::KeyN,
        "O" | "o" => Code::KeyO,
        "P" | "p" => Code::KeyP,
        "Q" | "q" => Code::KeyQ,
        "R" | "r" => Code::KeyR,
        "S" | "s" => Code::KeyS,
        "T" | "t" => Code::KeyT,
        "U" | "u" => Code::KeyU,
        "V" | "v" => Code::KeyV,
        "W" | "w" => Code::KeyW,
        "X" | "x" => Code::KeyX,
        "Y" | "y" => Code::KeyY,
        "Z" | "z" => Code::KeyZ,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        "Space" | " " => Code::Space,
        "Enter" | "Return" => Code::Enter,
        "Tab" => Code::Tab,
        "Esc" | "Escape" => Code::Escape,
        "Backspace" => Code::Backspace,
        "Delete" | "Del" => Code::Delete,
        "Home" => Code::Home,
        "End" => Code::End,
        "PageUp" => Code::PageUp,
        "PageDown" => Code::PageDown,
        "Up" => Code::ArrowUp,
        "Down" => Code::ArrowDown,
        "Left" => Code::ArrowLeft,
        "Right" => Code::ArrowRight,
        "Insert" => Code::Insert,
        "-" | "Minus" => Code::Minus,
        "=" | "Equal" => Code::Equal,
        "[" | "BracketLeft" => Code::BracketLeft,
        "]" | "BracketRight" => Code::BracketRight,
        ";" | "Semicolon" => Code::Semicolon,
        "'" | "Quote" => Code::Quote,
        "," | "Comma" => Code::Comma,
        "." | "Period" => Code::Period,
        "/" | "Slash" => Code::Slash,
        "\\" | "Backslash" => Code::Backslash,
        _ => return Err(format!("未知按键: {}", key_part)),
    };

    let mods = if mods.is_empty() { None } else { Some(mods) };
    Ok(Shortcut::new(mods, code))
}

fn register_read_shortcut(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut_str(shortcut_str)?;
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app_handle.emit("shortcut-triggered", ());
                let app_for_spawn = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_for_spawn.state::<AppState>();
                    let snapshot = {
                        let settings = match state.settings.lock() {
                            Ok(s) => s,
                            Err(e) => { eprintln!("[shortcut] settings lock: {}", e); return; }
                        };
                        (
                            settings.language.clone(),
                            clamp_quality(settings.quality) as usize,
                            clamp_speed(settings.speed),
                            clamp_silence(settings.silence),
                        )
                    };
                    let engine = {
                        let opt = match state.tts_engine.lock() {
                            Ok(o) => o,
                            Err(e) => { eprintln!("[shortcut] engine lock: {}", e); return; }
                        };
                        match opt.as_ref() {
                            Some(e) => Arc::clone(e),
                            None => { eprintln!("[shortcut] TTS engine not ready"); return; }
                        }
                    };
                    let player = Arc::clone(&state.audio_player);
                    let last_text = Arc::clone(&state.last_text);
                    drop(state);
                    if let Err(e) = run_read_from_clipboard(
                        app_for_spawn, engine, player, last_text,
                        snapshot.0, snapshot.1, snapshot.2, snapshot.3,
                    ).await {
                        eprintln!("[shortcut] read_selected_text failed: {}", e);
                    }
                });
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn register_pause_shortcut(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut_str(shortcut_str)?;
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let state = app_handle.state::<AppState>();
                let _ = state.audio_player.pause();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn setup_shortcuts(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    register_read_shortcut(app, &settings.shortcut)?;
    if !settings.pause_shortcut.trim().is_empty() {
        register_pause_shortcut(app, &settings.pause_shortcut)?;
    }
    Ok(())
}
