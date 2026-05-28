# supertonic-reader

闪电般快、完全本地的 TTS 划词朗读工具——基于 Tauri 2 + React + [Supertonic 3](https://github.com/supertone-inc/supertonic)，在 Windows 上通过 DirectML 利用本地 GPU 加速。

## 特性

- **划词朗读**：在任何应用中选中文字，按 `Ctrl + ~` 立即朗读，按 `Ctrl + 1` 暂停/继续
- **完全本地**：ONNX Runtime 推理，无网络请求
- **31 种朗读语言**：英语、中文、韩语、日语、法语、德语、西班牙语、俄语等
- **10 种预设音色**：M1–M5 / F1–F5
- **9 种界面语言**：英、简中、日、韩、西、法、马来、越、俄，右上角随时切换
- **GPU 加速**：在 Windows 上通过 DirectML EP 将 vector_estimator / vocoder 路由到本地 GPU（任何 DX12 设备），text_encoder / duration_predictor 留在 CPU
- **流式播放**：第一段合成完成立即开始播放，剪贴板文本在按下快捷键后立刻显示在界面上
- **主页可调参数**：质量、语速、停顿都是滑块，松手 ~300ms 后自动保存生效
- **托盘常驻**：关闭窗口最小化到系统托盘

## 技术栈

- **GUI**：Tauri 2 + React 18 + TypeScript + Tailwind + shadcn/ui
- **TTS**：Supertonic 3 + onnxruntime 1.19（通过 `ort` 2.0.0-rc.12，静态链接）
- **加速**：DirectML EP（vector_estimator / vocoder）
- **音频播放**：rodio（mpsc actor）

## 开发环境

- Rust 1.90.0（由 `rust-toolchain.toml` 锁定）
- Node.js ≥ 16
- Visual Studio 2022 Build Tools（含 C++ 桌面工作负载）
- Windows 10 1709+ / Windows 11（DirectML 系统依赖）

## 构建

```bash
npm install
npm run tauri:build
```

产物：

- 安装包：`src-tauri/target/release/bundle/nsis/supertonic-reader_*.exe`
- 裸 exe：`src-tauri/target/release/supertonic-reader.exe`（旁边带 `DirectML.dll`）

## 模型文件

首次运行时从 HuggingFace 自动下载（约 400MB），保存在 exe 同级目录的 `supertonic-reader-data/models/` 下。

也可手动放置：

```
<安装目录>/supertonic-reader-data/models/
├── onnx\
│   ├── duration_predictor.onnx
│   ├── text_encoder.onnx
│   ├── vector_estimator.onnx
│   ├── vocoder.onnx
│   ├── unicode_indexer.json
│   └── tts.json
└── voice_styles\
    ├── M1.json … M5.json
    └── F1.json … F5.json
```

来源：[HuggingFace - Supertone/supertonic-3](https://huggingface.co/Supertone/supertonic-3)

## 性能调优

通过环境变量控制：

| 变量 | 默认 | 说明 |
|---|---|---|
| `TTS_INTRA_THREADS` | `clamp(可用核数, 2, 4)` | ORT intra-op 线程数。CPU 占用敏感场景设 `2` 或 `1` |
| `TTS_DISABLE_DML` | 未设 | 设为 `1` 时强制走纯 CPU，绕过 DirectML EP |

## 朗读参数范围

| 参数 | 范围 | 默认 | 说明 |
|---|---|---|---|
| Quality（denoise 步数） | 2 – 16 | 8 | 步数越高音质越好、合成越慢，官方推荐值 |
| Speed | 0.80x – 1.30x | 1.05x | 超出该范围会出现明显伪声 |
| 句间停顿 | 0.30s – 1.00s | 0.30s | 仅长文本被自动分段时生效 |

主页就能拖动滑块改这三个值，松手 ~300ms 后自动保存。

## 默认快捷键

| 快捷键 | 动作 |
|---|---|
| `Ctrl + \`` | 朗读当前选中的文字 |
| `Ctrl + 1` | 暂停/继续当前播放 |

均可在「设置」页中修改。

## 卸载

通过 Windows 控制面板卸载时，安装向导会显示一个 **"Delete the application data"** 复选框：

- **不勾选**（默认）：保留 `supertonic-reader-data/`，下次重装无需重新下载 400MB 模型
- **勾选**：连同模型文件、settings、UI 偏好一并删除

## 协议

- 代码：MIT（或 Apache-2.0，由你决定）
- 模型：[OpenRAIL-M](https://huggingface.co/Supertone/supertonic-3/blob/main/LICENSE)（由 Supertone 发布，使用时需遵守）
