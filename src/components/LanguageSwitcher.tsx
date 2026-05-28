import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Globe } from "lucide-react";
import { SUPPORTED_UI_LANGS, type UiLang } from "@/i18n";

interface Props {
  className?: string;
  /** When provided, the switcher will also persist the new value into settings. */
  persistToSettings?: boolean;
}

const LABELS: Record<UiLang, string> = {
  en: "English",
  zh: "简体中文",
  ja: "日本語",
  ko: "한국어",
  es: "Español",
  fr: "Français",
  ms: "Bahasa Melayu",
  vi: "Tiếng Việt",
  ru: "Русский",
};

export default function LanguageSwitcher({ className = "", persistToSettings = true }: Props) {
  const { i18n } = useTranslation();
  const current = (SUPPORTED_UI_LANGS as readonly string[]).includes(i18n.language)
    ? (i18n.language as UiLang)
    : "en";

  const handleChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const next = e.target.value as UiLang;
    await i18n.changeLanguage(next);
    if (persistToSettings) {
      try {
        const current = await invoke<Record<string, unknown>>("get_settings");
        await invoke("save_settings_cmd", {
          settings: { ...current, uiLanguage: next },
        });
      } catch (err) {
        console.error("[LanguageSwitcher] persist failed:", err);
      }
    }
  };

  return (
    <div className={`flex items-center gap-1 ${className}`}>
      <Globe className="h-3.5 w-3.5 text-muted-foreground" />
      <select
        value={current}
        onChange={handleChange}
        className="appearance-none rounded-md border border-input bg-background px-2 py-1 text-xs hover:bg-muted focus:outline-none focus:ring-1 focus:ring-ring"
      >
        {SUPPORTED_UI_LANGS.map((code) => (
          <option key={code} value={code}>
            {LABELS[code]}
          </option>
        ))}
      </select>
    </div>
  );
}
