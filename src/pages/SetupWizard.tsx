import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke, Channel } from "@tauri-apps/api/core";
import { Slider } from "@/components/ui/slider";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import LanguageSwitcher from "@/components/LanguageSwitcher";
import { RotateCw, FolderOpen, Download, CheckCircle2, ExternalLink } from "lucide-react";

interface Props {
  onComplete: () => void;
}

interface TtsState {
  ready: boolean;
  downloading: boolean;
  progress: number;
  lastText: string;
}

const MIRROR_BASE = "https://hf-mirror.com/Supertone/supertonic-3/resolve/main";

const MODEL_FILES: { path: string; labelKey: string }[] = [
  { path: "onnx/duration_predictor.onnx", labelKey: "settings.modelFiles.duration_predictor" },
  { path: "onnx/text_encoder.onnx", labelKey: "settings.modelFiles.text_encoder" },
  { path: "onnx/vector_estimator.onnx", labelKey: "settings.modelFiles.vector_estimator" },
  { path: "onnx/vocoder.onnx", labelKey: "settings.modelFiles.vocoder" },
  { path: "onnx/unicode_indexer.json", labelKey: "settings.modelFiles.unicode_indexer" },
  { path: "onnx/tts.json", labelKey: "settings.modelFiles.tts_config" },
  { path: "voice_styles/M1.json", labelKey: "settings.modelFiles.M1" },
  { path: "voice_styles/M2.json", labelKey: "settings.modelFiles.M2" },
  { path: "voice_styles/M3.json", labelKey: "settings.modelFiles.M3" },
  { path: "voice_styles/M4.json", labelKey: "settings.modelFiles.M4" },
  { path: "voice_styles/M5.json", labelKey: "settings.modelFiles.M5" },
  { path: "voice_styles/F1.json", labelKey: "settings.modelFiles.F1" },
  { path: "voice_styles/F2.json", labelKey: "settings.modelFiles.F2" },
  { path: "voice_styles/F3.json", labelKey: "settings.modelFiles.F3" },
  { path: "voice_styles/F4.json", labelKey: "settings.modelFiles.F4" },
  { path: "voice_styles/F5.json", labelKey: "settings.modelFiles.F5" },
];

export default function SetupWizard({ onComplete }: Props) {
  const { t, i18n } = useTranslation();
  const [quality, setQuality] = useState([8]);
  const [speed, setSpeed] = useState([1.05]);
  const [silence, setSilence] = useState([0.3]);
  const [modelReady, setModelReady] = useState<boolean | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [showFileList, setShowFileList] = useState(false);

  useEffect(() => {
    checkModel();
  }, []);

  const checkModel = async () => {
    try {
      const state = await invoke<TtsState>("get_tts_state");
      setModelReady(state.ready);
      setDownloading(state.downloading);
      setDownloadProgress(state.progress);
    } catch {
      setModelReady(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setError("");
    try {
      await invoke("save_settings_cmd", {
        settings: {
          initialized: true,
          quality: quality[0],
          speed: speed[0],
          silence: silence[0],
          language: "en",
          voice_style: "M1",
          shortcut: "Ctrl+`",
          uiLanguage: i18n.language,
        },
      });
      if (modelReady) {
        onComplete();
      }
    } catch (e: any) {
      setError(typeof e === "string" ? e : e?.message || t("setup.errors.saveFailed"));
    } finally {
      setSaving(false);
    }
  };

  const handleDownload = async () => {
    setDownloading(true);
    setError("");
    setDownloadProgress(0);
    try {
      const channel = new Channel<number>();
      channel.onmessage = (progress) => {
        setDownloadProgress(progress);
      };
      await invoke("download_model_cmd", { channel });
      await checkModel();
    } catch (e: any) {
      setError(typeof e === "string" ? e : e?.message || t("setup.errors.downloadFailed"));
    } finally {
      setDownloading(false);
    }
  };

  const handleDownloadMirror = async () => {
    setDownloading(true);
    setError("");
    setDownloadProgress(0);
    try {
      const channel = new Channel<number>();
      channel.onmessage = (progress) => {
        setDownloadProgress(progress);
      };
      await invoke("download_model_mirror_cmd", { channel });
      await checkModel();
    } catch (e: any) {
      setError(typeof e === "string" ? e : e?.message || t("setup.errors.mirrorDownloadFailed"));
    } finally {
      setDownloading(false);
    }
  };

  const handleOpenModelDir = async () => {
    await invoke("open_model_directory");
  };

  const fileUrl = (path: string) => `${MIRROR_BASE}/${path}`;

  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-6">
      <Card className="w-full max-w-md">
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="text-xl">{t("setup.title")}</CardTitle>
          <LanguageSwitcher persistToSettings={false} />
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="space-y-2">
            <div className="flex justify-between">
              <Label>{t("setup.quality.label")}</Label>
              <span className="text-sm text-muted-foreground">{quality[0]}</span>
            </div>
            <Slider value={quality} onValueChange={setQuality} min={2} max={16} step={1} />
            <p className="text-xs text-muted-foreground">{t("setup.quality.hint")}</p>
          </div>

          <div className="space-y-2">
            <div className="flex justify-between">
              <Label>{t("setup.speed.label")}</Label>
              <span className="text-sm text-muted-foreground">{speed[0].toFixed(2)}x</span>
            </div>
            <Slider value={speed} onValueChange={setSpeed} min={0.8} max={1.3} step={0.05} />
            <p className="text-xs text-muted-foreground">{t("setup.speed.hint")}</p>
          </div>

          <div className="space-y-2">
            <div className="flex justify-between">
              <Label>{t("setup.silence.label")}</Label>
              <span className="text-sm text-muted-foreground">{silence[0].toFixed(2)}s</span>
            </div>
            <Slider value={silence} onValueChange={setSilence} min={0.3} max={1.0} step={0.05} />
            <p className="text-xs text-muted-foreground">{t("setup.silence.hint")}</p>
          </div>

          <div className="rounded-lg border p-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">{t("setup.model.status")}</span>
              <div className="flex items-center gap-2">
                {modelReady === null ? (
                  <Badge variant="outline">{t("setup.model.checking")}</Badge>
                ) : modelReady ? (
                  <Badge variant="default" className="gap-1">
                    <CheckCircle2 className="h-3 w-3" />
                    {t("setup.model.ready")}
                  </Badge>
                ) : (
                  <Badge variant="destructive">{t("setup.model.missing")}</Badge>
                )}
                <Button variant="ghost" size="icon" className="h-6 w-6" onClick={checkModel}>
                  <RotateCw className="h-3 w-3" />
                </Button>
              </div>
            </div>

            {modelReady === false && !downloading && (
              <div className="space-y-3">
                <p className="text-xs text-muted-foreground">
                  {t("setup.model.missingHint")}
                </p>
                <div className="grid grid-cols-2 gap-2">
                  <Button size="sm" onClick={handleDownload}>
                    <Download className="mr-1 h-3 w-3" />
                    {t("setup.model.autoDownload")}
                  </Button>
                  <Button size="sm" variant="secondary" onClick={handleDownloadMirror}>
                    <Download className="mr-1 h-3 w-3" />
                    {t("setup.model.mirrorDownload")}
                  </Button>
                </div>
                <Button size="sm" variant="outline" className="w-full" onClick={handleOpenModelDir}>
                  <FolderOpen className="mr-1 h-3 w-3" />
                  {t("setup.model.manualPlace")}
                </Button>

                <div className="rounded bg-muted p-3 space-y-2">
                  <button
                    className="flex w-full items-center justify-between text-xs font-medium"
                    onClick={() => setShowFileList(!showFileList)}
                  >
                    <span>{t("setup.model.perFileTitle")}</span>
                    <span>{showFileList ? t("setup.model.collapse") : t("setup.model.expand")}</span>
                  </button>

                  {showFileList && (
                    <div className="space-y-2">
                      <p className="text-xs text-muted-foreground">
                        {t("setup.model.perFileHint")}
                      </p>
                      <div className="max-h-48 overflow-y-auto rounded border bg-background">
                        {MODEL_FILES.map((f) => (
                          <button
                            key={f.path}
                            className="flex w-full items-center justify-between px-2 py-1.5 text-xs hover:bg-secondary border-b last:border-b-0 text-left"
                            title={f.path}
                            onClick={() => invoke("open_url", { url: fileUrl(f.path) })}
                          >
                            <span className="truncate">{t(f.labelKey)}</span>
                            <ExternalLink className="ml-2 h-3 w-3 shrink-0 text-muted-foreground" />
                          </button>
                        ))}
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {t("setup.model.perFileTip")}
                      </p>
                      <pre className="text-[10px] bg-background p-2 rounded overflow-x-auto leading-relaxed border">
{`models/
├── onnx/
│   ├── duration_predictor.onnx
│   ├── text_encoder.onnx
│   ├── vector_estimator.onnx
│   ├── vocoder.onnx
│   ├── unicode_indexer.json
│   └── tts.json
└── voice_styles/
    ├── M1.json ~ M5.json
    └── F1.json ~ F5.json`}
                      </pre>
                      <Button size="sm" variant="ghost" className="w-full" onClick={handleOpenModelDir}>
                        <FolderOpen className="mr-1 h-3 w-3" />
                        {t("setup.model.openDir")}
                      </Button>
                    </div>
                  )}
                </div>
              </div>
            )}

            {downloading && (
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{t("setup.model.downloading")}</span>
                  <span>{downloadProgress}%</span>
                </div>
                <div className="h-2 w-full overflow-hidden rounded-full bg-secondary">
                  <div
                    className="h-full bg-primary transition-all"
                    style={{ width: `${Math.min(downloadProgress, 100)}%` }}
                  />
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("setup.model.downloadingHint")}
                </p>
              </div>
            )}
          </div>

          {error && (
            <div className="rounded bg-destructive/10 p-3 text-sm text-destructive">
              {error}
            </div>
          )}

          <Button
            className="w-full"
            onClick={handleSave}
            disabled={saving || !modelReady}
          >
            {saving
              ? t("common.saving")
              : modelReady
                ? t("setup.startButton.start")
                : t("setup.startButton.needsModel")}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
