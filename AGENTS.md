# AGENTS.md

## Project Overview

`supertonic-reader` — a Windows desktop TTS reader for selected text, built on
Tauri 2 + React + Supertonic 3 (ONNX). Press `Ctrl + ~` while text is selected
to speak it. All inference is local; the model runs on CPU and can route the
two heavy graphs to the local GPU via DirectML.

## Technology Stack

- **GUI**: Tauri 2 (Rust backend + WebView2 frontend)
- **Frontend**: React 18 + TypeScript + Tailwind + shadcn/ui
- **TTS**: Supertonic 3 via `ort` 2.0.0-rc.12, statically linked onnxruntime 1.19
- **Hardware accel (Windows)**: DirectML EP routes `vector_estimator` and
  `vocoder` to the local GPU; `text_encoder` / `duration_predictor` stay on CPU
- **Audio playback**: rodio behind a `std::sync::mpsc` actor (rodio handles
  aren't `Send + Sync`, so the player owns a `Sender<AudioCommand>`)
- **Streaming**: chunks play as soon as the first one is synthesized; later
  chunks are appended to the same sink

## Project Structure

```
supertonic-reader/
├── src/                       # React frontend
│   ├── components/
│   │   ├── ui/                # shadcn/ui primitives + custom Slider
│   │   └── LanguageSwitcher.tsx
│   ├── i18n/
│   │   ├── index.ts           # i18next bootstrap + SUPPORTED_UI_LANGS
│   │   └── locales/           # en / zh / ja / ko / es / fr / ms / vi / ru
│   ├── pages/                 # SetupWizard / MainPanel / Settings
│   ├── App.tsx                # mounts <Toaster>, applies saved uiLanguage
│   └── main.tsx
├── src-tauri/                 # Tauri Rust backend
│   ├── src/
│   │   ├── main.rs / lib.rs   # Tauri commands, tray, global shortcuts
│   │   ├── audio.rs           # mpsc audio actor (Play / Append / …)
│   │   ├── config.rs          # AppSettings JSON (incl. ui_language)
│   │   ├── model_manager.rs   # HuggingFace download + integrity check
│   │   └── tts/
│   │       ├── mod.rs         # `TtsEngine` wrapping `TextToSpeech`
│   │       └── helper.rs      # `build_session` (CPU/DML), `_infer`, chunking
│   ├── binaries/
│   │   └── DirectML.dll       # vendored, copied next to exe at install
│   ├── installer-hooks.nsi    # NSIS PREUNINSTALL hook for data dir cleanup
│   ├── Cargo.toml
│   └── tauri.conf.json
├── dist/                      # Vite build output (gitignored)
├── package.json
├── tailwind.config.js
└── rust-toolchain.toml        # Rust 1.90.0
```

## Build Requirements

- **Rust** 1.90.0 (pinned in `rust-toolchain.toml`). `ort` 2.0.0-rc.12 needs
  Rust ≥ 1.88; rc.9 is broken on Rust 1.95+.
- **Node.js** 16+ (Vite 4)
- **MSVC** Visual Studio 2022 Build Tools with the C++ desktop workload
- **Windows** 10 1709+ / 11 (DirectML system dependency)

## Build Commands

```bash
# Frontend only
npm install
npm run build

# Full app (frontend + Tauri release bundle)
npm run tauri:build
```

Artifacts:
- NSIS installer: `src-tauri/target/release/bundle/nsis/supertonic-reader_*.exe`
- Bare exe: `src-tauri/target/release/supertonic-reader.exe` (with `DirectML.dll` next to it)

## Key Implementation Notes

1. **ort feature set**: `ort = { default-features = false, features =
   ["std", "ndarray", "download-binaries", "tls-native", "copy-dylibs",
   "directml"] }`.
   - **Do not** add `load-dynamic` — under rc.12 it implies
     `ort-sys/disable-linking`, which **skips downloading `onnxruntime.dll`
     entirely**. Static linking + `copy-dylibs` is the only path that works.
   - `std` must be listed explicitly because we don't pull `ort`'s default
     features; without it `commit_from_file` is gated out.

2. **Graph optimization level**: capped at `Level2` (`ORT_ENABLE_EXTENDED`).
   `Level3` maps to `ORT_ENABLE_LAYOUT` in rc.12, which doesn't exist in the
   bundled onnxruntime 1.19 (1.20+ only) — the runtime rejects it with
   "graph_optimization_level is not valid". DML path uses `Level1`
   (`ORT_ENABLE_BASIC`); higher is rejected by DML EP.

3. **DirectML routing** (`src-tauri/src/tts/helper.rs:build_session`):
   - `SessionKind::VectorEstimator` / `Vocoder` → DML EP, `Level1`, no memory
     pattern (both required by DML)
   - `SessionKind::DurationPredictor` / `TextEncoder` → CPU, `Level2`, memory
     pattern on
   - DML registration falls back to CPU automatically if the EP isn't available
   - `TTS_DISABLE_DML=1` skips the DML path entirely

4. **Threading**: `intra_threads = clamp(available_parallelism, 2, 4)` by
   default; `inter_threads = 1` and `parallel_execution = false` since each
   call is single-sequence. Override with `TTS_INTRA_THREADS`.

5. **Denoise loop hot path** (`helper.rs:_infer`): only `current_step` is
   rebuilt each iteration. All other inputs (`text_emb`, `style.ttl`,
   `text_mask`, `latent_mask`, `total_step`, `text_ids`) are constructed once
   and passed as `TensorRef::from_array_view(arr.view())` — zero-copy
   borrowing of the same ndarray.

6. **Streaming playback** (`lib.rs:read_selected_text` →
   `helper.rs:call_streaming` → `audio.rs:AudioCommand::Append`): each chunk
   is synthesized and pushed into the audio sink before the next chunk
   starts, so playback latency = time-to-first-chunk, not total synthesis
   time.

7. **Audio actor**: `rodio::OutputStream` and `Sink` are not `Send + Sync`,
   so they live on a dedicated thread. `AudioPlayer` only holds a
   `Sender<AudioCommand>`. Commands: `Play` (replace), `Append` (queue),
   `Stop`, `Pause`, `Resume`.

8. **Model files**: Supertonic 3 weights (~400MB) are NOT bundled. On first
   run users can:
   - Auto-download from HuggingFace (`Supertone/supertonic-3` or
     `hf-mirror.com` mirror)
   - Manually drop files into `<install-dir>/supertonic-reader-data/models/`

9. **Async TTS commands**: `read_selected_text` and `test_tts_cmd` are
   `async fn` Tauri commands. They snapshot the needed `Arc`s
   (`Arc<TtsEngine>`, `Arc<AudioPlayer>`, `Arc<Mutex<String>>` for
   `last_text`) under the outer `Mutex<Option<…>>` then release the lock,
   and hand the heavy ONNX work to `tokio::task::spawn_blocking`. Without
   this the IPC main thread blocks for the entire inference (~seconds) and
   the whole UI freezes — including `get_tts_state`, settings save, and
   pause/stop commands.

10. **Voice-style hot swap**: `TtsEngine::style` is a `Mutex<Style>`.
    `synthesize_streaming` snapshots it under the style lock and releases
    before grabbing the inner `tts` lock, so a `save_settings_cmd` voice
    change during a long synthesis only blocks for the snapshot, not the
    whole inference. `replace_style` updates the engine in place via the
    inner Mutex — no Arc swap needed.

11. **Immediate clipboard text feedback**: after the clipboard read in
    `run_read_from_clipboard`, the captured text is written to
    `last_text` and broadcast via `tts-text-captured` event **before**
    spawning the synthesis. `MainPanel` listens for that event and shows
    the snippet immediately, so users see what's about to be read without
    waiting for the audio.

12. **DirectML.dll bundling**: ort's build script downloads
    `DirectML.dll` into the local cargo cache and symlinks it next to the
    dev exe via `copy-dylibs`. The symlink is per-machine and can't ship
    in the NSIS installer, so a real copy is checked in at
    `src-tauri/binaries/DirectML.dll`. `tauri.conf.json` uses the
    **map form** of `bundle.resources`:
    ```json
    "resources": { "binaries/DirectML.dll": "DirectML.dll" }
    ```
    Array form would land the DLL in `resources/` (where the OS DLL loader
    won't find it). Map form puts it directly next to the exe at install
    time. MSI bundler has a bug with this; NSIS works.

13. **Uninstaller cleanup** (`src-tauri/installer-hooks.nsi`): the Tauri
    NSIS template already renders a "Delete the application data"
    checkbox and stores its state in `$DeleteAppDataCheckboxState`. The
    template's default cleanup targets `%APPDATA%\<bundle-id>` /
    `%LOCALAPPDATA%\<bundle-id>`, which we don't use. Our hook reads the
    same variable: if ticked, `RMDir /r "$INSTDIR\supertonic-reader-data"`;
    otherwise leave it for re-install reuse. **Don't** add your own
    `MessageBox` confirmation — it duplicates the checkbox UX.

14. **i18n** (`src/i18n/`): react-i18next with 9 locales
    (`en / zh / ja / ko / es / fr / ms / vi / ru`). Default is `en`; users
    can switch from the top-right `LanguageSwitcher` on SetupWizard and
    MainPanel. The selected language is persisted as `uiLanguage` in
    settings — note the serde rename: Rust field is `ui_language`,
    JSON / frontend wire format is `uiLanguage`. `LANGUAGES` array in
    `Settings.tsx` (the TTS model languages) is intentionally NOT i18n —
    those are model language codes, translating their display names just
    confuses voice selection.

15. **Toast notifications** (`sonner`): `<Toaster position="bottom-right"
    richColors closeButton />` mounts in `App.tsx`. Use `toast.success(t(…))`
    / `toast.error(t(…), { description })`. Silent settings persistence
    (`MainPanel` slider debounce) deliberately does NOT toast on success
    — only on failure.

16. **MainPanel slider debounce**: synthesis params (quality / speed /
    silence) are editable on the main page and persisted automatically.
    The implementation uses 300 ms debounce via `useRef<setTimeout>` —
    not `onValueCommit`, because the custom Slider's pointerup / blur
    events don't fire reliably when the user drags out of the input
    range. Every `onValueChange` resets the timer; latest snapshot in
    `latestSettings.current` is what eventually gets sent.

## Frontend ↔ backend wire formats

Most fields are snake_case (`voice_style`, `pause_shortcut`). Two
exceptions kept as camelCase for JS ergonomics:
- `uiLanguage` (Rust: `ui_language`, serde renamed)
- The `Channel<u64>` progress channel passed to `download_model_cmd`

`save_settings_cmd` accepts the full `AppSettings` shape every time —
the frontend reads current settings, merges the change, and sends the
whole object back.

## Development Conventions

- `helper.rs` is adapted from the official Supertonic Rust example; keep
  divergences minimal and document them above
- All Tauri commands return `Result<T, String>` so the frontend can surface
  errors with `try { … } catch (e) { showToast(e) }`
- Don't introduce CUDA / TensorRT features — DirectML covers all DX12 GPUs
  including NVIDIA / AMD / Intel / iGPUs and avoids the CUDA toolchain
- Don't rename the data dir `supertonic-reader-data/models/` (next to the
  exe) without a migration path — existing users would lose already-downloaded
  models on update
