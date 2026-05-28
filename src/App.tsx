import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Toaster } from "sonner";
import i18n, { isUiLang } from "./i18n";
import SetupWizard from "./pages/SetupWizard";
import MainPanel from "./pages/MainPanel";
import Settings from "./pages/Settings";

function App() {
  const [initialized, setInitialized] = useState<boolean | null>(null);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    const setupListener = async () => {
      const unlisten = await listen("open-settings", () => {
        setShowSettings(true);
      });
      return unlisten;
    };
    const unlistenPromise = setupListener();

    const checkInit = async () => {
      try {
        const s = (await invoke("get_settings")) as Record<string, unknown>;
        const ui = (s as any).uiLanguage ?? (s as any).ui_language;
        if (typeof ui === "string" && isUiLang(ui) && ui !== i18n.language) {
          await i18n.changeLanguage(ui);
        }
        setInitialized(!!s.initialized);
      } catch (e) {
        console.error("[App] get_settings failed:", e);
        setInitialized(false);
      }
    };
    checkInit();

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  if (initialized === null) {
    return (
      <>
        <Toaster position="bottom-right" richColors closeButton />
        <div className="flex h-screen w-screen items-center justify-center bg-background">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
        </div>
      </>
    );
  }

  return (
    <>
      <Toaster position="bottom-right" richColors closeButton />
      {showSettings ? (
        <Settings onClose={() => setShowSettings(false)} />
      ) : initialized ? (
        <MainPanel />
      ) : (
        <SetupWizard onComplete={() => setInitialized(true)} />
      )}
    </>
  );
}

export default App;
