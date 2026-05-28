# CLAUDE.md

Project-specific notes for Claude Code. Read `AGENTS.md` first for the full
architecture overview — this file only lists pitfalls and conventions that
have already cost a build cycle to figure out.

## Build pitfalls — do not retry these

### 1. ort feature set is fragile

Use exactly:

```toml
ort = { version = "2.0.0-rc.12", default-features = false, features = [
  "std", "ndarray", "download-binaries", "tls-native", "copy-dylibs", "directml"
] }
```

**Do not add `load-dynamic`.** In rc.12 it transitively enables
`ort-sys/disable-linking`, which makes the build script return early **before
downloading `onnxruntime.dll`**. The exe links fine but crashes on startup
with no log because the DLL isn't next to it.

**Do not omit `std`.** Without it `SessionBuilder::commit_from_file` is gated
out (it's behind `#[cfg(feature = "std")]`), and the build fails with
"no method named `commit_from_file`".

### 2. GraphOptimizationLevel ceiling is Level2

`GraphOptimizationLevel::Level3` in ort 2.0.0-rc.12 maps to
`ORT_ENABLE_LAYOUT`, which is **onnxruntime 1.20+ only**. The bundled
runtime via `download-binaries` is **1.19.x** — it rejects the value at
session-create time with `"graph_optimization_level is not valid"`. The
engine appears to "fail to load the model" but it's actually a
config-rejection error.

Use `Level2` for CPU sessions, `Level1` for DML sessions (DML rejects
anything above Basic).

### 3. DirectML EP has hard constraints

When attaching the DirectML EP to a `SessionBuilder`:

- `optimization_level` must be `Level1` or lower
- `memory_pattern` must be `false`
- `parallel_execution` should stay `false`

If any of these are wrong the registration call returns an error. The code in
`helper.rs:build_session` enforces this; don't lift the level inside the DML
branch.

### 4. ort-sys build cache is sticky

`cargo clean` doesn't touch the `~/AppData/Local/ort.pyke.io/dfbin/` cache,
and the per-target `target/release/build/ort-sys-*/output` files are tiny
(38 bytes) when the build script early-returned. If `target/release/` is
missing `DirectML.dll` after a build:

```bash
cd src-tauri && cargo clean -p ort-sys -p ort
```

Then rebuild. Don't try to "fix" things by deleting the dfbin cache by hand.

### 5. `bundle.resources` must be the map form

Tauri's `bundle.resources` accepts both array and map syntax. **Array** form
lands files inside `resources/` relative to the exe, which the OS DLL loader
won't search:

```json
"resources": ["binaries/DirectML.dll"]   // ❌ lands in resources/DirectML.dll
```

**Map** form places the file at the destination relative to the exe:

```json
"resources": { "binaries/DirectML.dll": "DirectML.dll" }   // ✅ next to exe
```

The exe needs `DirectML.dll` next to it for the DML EP to load, so the map
form is the only one that works. This is NSIS-only — the MSI bundler has a
bug ([tauri#12391](https://github.com/tauri-apps/tauri/issues/12391)) where
root-level DLL targets get dropped during MSI build.

### 6. Inference must be async + `spawn_blocking`

Tauri IPC runs commands on a small async runtime. If a command is a
synchronous `fn` and does multi-second work (ONNX inference here), it blocks
the entire IPC bus — every other webview ↔ rust call queues behind it, and
the UI freezes solid until the work returns.

Fix pattern (see `lib.rs:read_selected_text` / `run_read_from_clipboard`):

1. Command is `async fn` returning `Result<_, String>`
2. Inside, snapshot the needed `Arc<…>` handles under their Mutexes, then
   release the locks immediately
3. Hand the actual work to `tokio::task::spawn_blocking(move || { … })`
   and `.await` it
4. The audio callback runs on the blocking thread; mpsc `Sender::send` is
   `Send + Sync`, so it works fine across threads

Don't try to make it work with `tauri::async_runtime::spawn` directly — its
futures need `'static`, and `tauri::State<'_, T>` is not `'static`. Snapshot
out of `State` while still in the command body.

### 7. Custom Slider's commit events are unreliable

`src/components/ui/slider.tsx` is a custom wrapper around a native
`<input type="range">` (not Radix). I added an `onValueCommit` prop that
listens for `pointerup` / `keyup` / `blur`, but in practice the user can
drag, slide outside the input rect, and release — none of those events fire
on the input.

Use `onValueChange` + 300 ms debounce instead. See
`MainPanel.tsx:schedulePersist` — `useRef<setTimeout>` is reset on every
change, the latest snapshot lives in another `useRef` so the closure isn't
stale.

### 8. Uninstaller hook: reuse the built-in checkbox

Tauri's NSIS template already renders "Delete the application data" and
stores the state in `$DeleteAppDataCheckboxState`. Read it in
`NSIS_HOOK_PREUNINSTALL`; **don't** add your own confirm dialog. The
template's own cleanup hits `%APPDATA%\<bundle-id>` /
`%LOCALAPPDATA%\<bundle-id>`, which we don't use, so adding *just* a hook
that reads the checkbox state is the right pattern.

## Performance work that was already done

Don't repeat these as if they're new:

- `helper.rs:_infer` denoise loop **already uses `TensorRef::from_array_view`**
  for all loop-invariant inputs. Don't add `.clone()` back.
- `sample_noisy_latent` **already** generates noise into a single contiguous
  Vec, then zeros out the masked tail. Don't reintroduce the three nested
  loops.
- Long-text playback **already** streams chunk-by-chunk through
  `audio.rs:AudioCommand::Append`. Don't add a "collect everything then
  play" path unless explicitly asked.
- Inference **already** runs in `tokio::task::spawn_blocking` — don't move
  it back onto the IPC thread.
- `TtsEngine::style` is **already** a `Mutex<Style>` with a `replace_style`
  method, so voice-style hot swap doesn't need to rebuild the engine.

## What NOT to change without asking

- **Data directory `supertonic-reader-data/models/` (next to the exe)**: tied
  to the official Supertonic HuggingFace tree. Renaming files breaks
  auto-download; renaming the directory itself orphans already-downloaded
  models on user machines unless paired with a migration shim.
- **`Mutex<Option<Arc<TtsEngine>>>` shape in `AppState`**: looks redundant
  next to the inner mutexes (`tts`, `style`), but the outer Mutex lets us
  hot-swap the engine during model re-download, while `Arc` lets commands
  clone a handle and release the outer lock before doing long work.
- **`uiLanguage` / `ui_language` rename** in `AppSettings`: the serde
  rename is what lets old configs (with the snake_case key) keep working
  alongside the camelCase wire format. Don't unify them in one direction.
- **Official Supertonic parameter ranges**: quality is **2–16** (denoise
  steps), speed is **0.8–1.3**. These come from the upstream Supertonic
  3 model — picking values outside the official range produces audible
  artifacts. `lib.rs:clamp_quality` / `clamp_speed` enforce this; don't
  widen the bounds.
- **`LANGUAGES` array in `Settings.tsx`** (the 31 TTS model languages):
  these are model language codes, displayed as e.g. `Korean (한국어)`.
  Don't translate the labels — users need to find their own language
  visually, not in their UI language.
- **Tauri identifier `com.tts-app.app`**: keeping it stable preserves
  update channels for existing installs even though `tts-app` is no
  longer the user-visible name.

## Naming

`supertonic-reader` is the canonical name everywhere now — Cargo package, npm
package, exe filename (`supertonic-reader.exe`), NSIS installer, window
title, README. The data dir is `supertonic-reader-data/` next to the exe.
The Tauri identifier is still `com.tts-app.app` (changing it would break
update channels for existing installs).

## Build & test cadence

- Frontend-only iteration: `npm run dev`
- Full app verification: `npm run tauri:build` (release; cached rebuild
  ~40 s, first build downloads ort binaries ~5 min)
- There is no test suite yet. After non-trivial Rust changes, run at least
  `cd src-tauri && cargo check` before reporting "done".
