import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Slider } from "@/components/ui/slider";
import { Label } from "@/components/ui/label";
import LanguageSwitcher from "@/components/LanguageSwitcher";

interface SettingsData {
  initialized?: boolean;
  language: string;
  voice_style: string;
  quality: number;
  speed: number;
  silence: number;
  shortcut: string;
  pause_shortcut: string;
  uiLanguage?: string;
}

interface TtsState {
  ready: boolean;
  downloading: boolean;
  progress: number;
  lastText: string;
  error: string;
}

function clampQuality(q: number): number {
  return Math.max(2, Math.min(16, Math.round(q)));
}
function clampSpeed(s: number): number {
  return Math.round(Math.max(0.8, Math.min(1.3, s)) * 100) / 100;
}
function clampSilence(s: number): number {
  return Math.round(Math.max(0.3, Math.min(1.0, s)) * 100) / 100;
}

export default function MainPanel() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [state, setState] = useState<TtsState>({
    ready: false,
    downloading: false,
    progress: 0,
    lastText: "",
    error: "",
  });
  const persistTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestSettings = useRef<SettingsData | null>(null);

  const refreshState = async () => {
    const s = await invoke<TtsState>("get_tts_state");
    setState(s);
  };

  const refreshSettings = async () => {
    const s = await invoke<SettingsData>("get_settings");
    setSettings(s);
    latestSettings.current = s;
  };

  useEffect(() => {
    refreshSettings();
    refreshState();
    const interval = setInterval(refreshState, 1000);
    const unlistenPromise = listen<string>("tts-text-captured", (event) => {
      setState((prev) => ({ ...prev, lastText: event.payload }));
    });
    return () => {
      clearInterval(interval);
      unlistenPromise.then((unlisten) => unlisten());
      if (persistTimer.current) clearTimeout(persistTimer.current);
    };
  }, []);

  const persist = async (next: SettingsData) => {
    try {
      await invoke("save_settings_cmd", { settings: { ...next, initialized: true } });
    } catch (e: any) {
      const message = typeof e === "string" ? e : e?.message || "save failed";
      toast.error(t("settings.toast.saveFailed"), { description: message });
    }
  };

  const schedulePersist = () => {
    if (persistTimer.current) clearTimeout(persistTimer.current);
    persistTimer.current = setTimeout(() => {
      if (latestSettings.current) persist(latestSettings.current);
    }, 300);
  };

  const changeQuality = (val: number) => {
    if (!settings) return;
    const v = clampQuality(val);
    if (v === settings.quality) return;
    const next = { ...settings, quality: v };
    setSettings(next);
    latestSettings.current = next;
    schedulePersist();
  };
  const changeSpeed = (val: number) => {
    if (!settings) return;
    const v = clampSpeed(val);
    if (v === settings.speed) return;
    const next = { ...settings, speed: v };
    setSettings(next);
    latestSettings.current = next;
    schedulePersist();
  };
  const changeSilence = (val: number) => {
    if (!settings) return;
    const v = clampSilence(val);
    if (v === settings.silence) return;
    const next = { ...settings, silence: v };
    setSettings(next);
    latestSettings.current = next;
    schedulePersist();
  };

  const handleReadClipboard = async () => {
    try {
      await invoke("read_selected_text");
    } catch (e: any) {
      toast.error(t("main.errors.readFailed"), {
        description: typeof e === "string" ? e : e?.message,
      });
    }
  };

  const handleOpenSettings = async () => {
    await invoke("open_settings_window");
  };

  const handleOpenModelDir = async () => {
    await invoke("open_model_directory");
  };

  const handleDownloadModel = async () => {
    const channel = new Channel<number>();
    channel.onmessage = (progress) => {
      setState((prev) => ({ ...prev, progress }));
    };
    try {
      await invoke("download_model_cmd", { channel });
      await refreshState();
    } catch (e: any) {
      toast.error(t("main.errors.downloadFailed"), {
        description: typeof e === "string" ? e : e?.message,
      });
    }
  };

  return (
    <div className="flex h-screen w-screen flex-col bg-background p-4">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-lg font-semibold">{t("app.title")}</h1>
        <div className="flex items-center gap-2">
          <LanguageSwitcher />
          <Button variant="outline" size="sm" onClick={handleOpenSettings}>
            {t("main.openSettings")}
          </Button>
          <Button variant="outline" size="sm" onClick={handleOpenModelDir}>
            {t("main.openModelDir")}
          </Button>
        </div>
      </div>

      <Card className="mb-4">
        <CardHeader className="pb-3">
          <CardTitle className="text-sm">{t("main.params.title")}</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-3">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-xs text-muted-foreground">{t("main.stats.quality")}</Label>
              <span className="text-sm font-medium">{settings?.quality ?? "-"}</span>
            </div>
            <Slider
              value={[settings?.quality ?? 8]}
              onValueChange={(v) => changeQuality(v[0])}
              min={2}
              max={16}
              step={1}
            />
          </div>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-xs text-muted-foreground">{t("main.stats.speed")}</Label>
              <span className="text-sm font-medium">{(settings?.speed ?? 1.05).toFixed(2)}x</span>
            </div>
            <Slider
              value={[settings?.speed ?? 1.05]}
              onValueChange={(v) => changeSpeed(v[0])}
              min={0.8}
              max={1.3}
              step={0.05}
            />
          </div>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-xs text-muted-foreground">{t("main.stats.silence")}</Label>
              <span className="text-sm font-medium">{(settings?.silence ?? 0.3).toFixed(2)}s</span>
            </div>
            <Slider
              value={[settings?.silence ?? 0.3]}
              onValueChange={(v) => changeSilence(v[0])}
              min={0.3}
              max={1.0}
              step={0.05}
            />
          </div>
        </CardContent>
      </Card>

      <Card className="mb-4 flex-1">
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle className="text-sm">{t("main.status.title")}</CardTitle>
          <Badge variant={state.ready ? "default" : "destructive"}>
            {state.ready
              ? t("main.status.ready")
              : state.downloading
                ? t("main.status.downloading")
                : t("main.status.missing")}
          </Badge>
        </CardHeader>
        <CardContent className="space-y-3">
          {state.downloading && (
            <div>
              <div className="mb-1 text-sm">
                {t("main.status.downloadingProgress", { progress: state.progress })}
              </div>
              <div className="h-2 w-full overflow-hidden rounded-full bg-secondary">
                <div
                  className="h-full bg-primary transition-all"
                  style={{ width: `${state.progress}%` }}
                />
              </div>
            </div>
          )}

          {!state.ready && !state.downloading && (
            <div className="space-y-2">
              <p className="text-sm text-muted-foreground">
                {state.error
                  ? t("main.status.engineError", { error: state.error })
                  : t("main.status.missingHint")}
              </p>
              <div className="flex gap-2">
                <Button size="sm" onClick={handleDownloadModel}>
                  {t("main.status.autoDownload")}
                </Button>
                <Button variant="outline" size="sm" onClick={handleOpenModelDir}>
                  {t("main.status.manualPlace")}
                </Button>
              </div>
            </div>
          )}

          {state.ready && (
            <div className="space-y-2">
              <p className="text-sm text-muted-foreground">
                {t("main.status.readyHint")}
              </p>
              <Button size="sm" onClick={handleReadClipboard}>
                {t("main.status.testRead")}
              </Button>
            </div>
          )}

          {state.lastText && (
            <div className="rounded-md bg-muted p-3">
              <div className="mb-1 text-xs text-muted-foreground">{t("main.status.recentRead")}</div>
              <div className="text-sm">{state.lastText}</div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
