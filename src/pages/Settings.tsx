import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Volume2 } from "lucide-react";

interface SettingsData {
  language: string;
  voice_style: string;
  quality: number;
  speed: number;
  silence: number;
  shortcut: string;
  pause_shortcut: string;
}

interface Props {
  onClose: () => void;
}

const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "ko", name: "Korean (한국어)" },
  { code: "ja", name: "Japanese (日本語)" },
  { code: "es", name: "Spanish (Español)" },
  { code: "fr", name: "French (Français)" },
  { code: "de", name: "German (Deutsch)" },
  { code: "it", name: "Italian (Italiano)" },
  { code: "pt", name: "Portuguese" },
  { code: "ru", name: "Russian (Русский)" },
  { code: "vi", name: "Vietnamese" },
  { code: "ar", name: "Arabic (العربية)" },
  { code: "hi", name: "Hindi (हिन्दी)" },
  { code: "id", name: "Indonesian" },
  { code: "tr", name: "Turkish" },
  { code: "pl", name: "Polish" },
  { code: "bg", name: "Bulgarian" },
  { code: "cs", name: "Czech" },
  { code: "da", name: "Danish" },
  { code: "el", name: "Greek" },
  { code: "et", name: "Estonian" },
  { code: "fi", name: "Finnish" },
  { code: "hr", name: "Croatian" },
  { code: "hu", name: "Hungarian" },
  { code: "lt", name: "Lithuanian" },
  { code: "lv", name: "Latvian" },
  { code: "nl", name: "Dutch" },
  { code: "ro", name: "Romanian" },
  { code: "sk", name: "Slovak" },
  { code: "sl", name: "Slovenian" },
  { code: "sv", name: "Swedish" },
  { code: "uk", name: "Ukrainian" },
];

const VOICE_STYLE_CODES = ["M1", "M2", "M3", "M4", "M5", "F1", "F2", "F3", "F4", "F5"] as const;

const TEST_TEXTS: Record<string, string> = {
  en: "I hope there is no war in this world.",
  ko: "이 세상에 전쟁이 없었으면 좋겠어요.",
  ja: "この世界に戦争がないことを願っています。",
  es: "Espero que no haya guerra en este mundo.",
  fr: "J'espère qu'il n'y a pas de guerre dans ce monde.",
  de: "Ich hoffe, es gibt keinen Krieg in dieser Welt.",
  it: "Spero che non ci sia guerra in questo mondo.",
  pt: "Espero que não haja guerra neste mundo.",
  ru: "Я надеюсь, что в этом мире нет войны.",
  vi: "Tôi mong rằng thế giới này không có chiến tranh.",
  ar: "أتمنى أن لا يكون هناك حرب في هذا العالم.",
  hi: "मुझे आशा है कि इस दुनिया में कोई युद्ध नहीं है।",
  id: "Saya berharap tidak ada perang di dunia ini.",
  tr: "Umarım bu dünyada savaş yoktur.",
  pl: "Mam nadzieję, że na tym świecie nie ma wojny.",
  bg: "Надявам се, че в този свят няма война.",
  cs: "Doufám, že na tomto světě není válka.",
  da: "Jeg håber, at der ikke er krig i denne verden.",
  el: "Ελπίζω να μην υπάρχει πόλεμος σε αυτόν τον κόσμο.",
  et: "Ma loodan, et selles maailmas pole sõda.",
  fi: "Toivon, että tässä maailmassa ei ole sotaa.",
  hr: "Nadam se da u ovom svijetu nema rata.",
  hu: "Remélem, hogy nincs háború ebben a világban.",
  lt: "Tikiuosi, kad šiame pasaulyje nėra karo.",
  lv: "Es ceru, ka šajā pasaulē nav kara.",
  nl: "Ik hoop dat er geen oorlog is in deze wereld.",
  ro: "Sper că nu există război în această lume.",
  sk: "Dúfam, že na tomto svete nie je vojna.",
  sl: "Upam, da na tem svetu ni vojne.",
  sv: "Jag hoppas att det inte finns krig i denna värld.",
  uk: "Я сподіваюся, що в цьому світі немає війни.",
};

function getTestText(lang: string): string {
  return TEST_TEXTS[lang] || TEST_TEXTS["en"];
}

export default function Settings({ onClose }: Props) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    invoke<SettingsData>("get_settings")
      .then((s) => setSettings(s))
      .catch((e) => {
        console.error(t("settings.errors.loadFailed"), e);
        setSettings({
          language: "en",
          voice_style: "M1",
          quality: 8,
          speed: 1.05,
          silence: 0.3,
          shortcut: "Ctrl+`",
          pause_shortcut: "Ctrl+1",
        });
      });
  }, [t]);

  const update = (partial: Partial<SettingsData>) => {
    setSettings((prev) => (prev ? { ...prev, ...partial } : null));
  };

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await invoke("save_settings_cmd", {
        settings: { ...settings, initialized: true },
      });
      toast.success(t("settings.toast.saved"));
    } catch (e: any) {
      const message = typeof e === "string" ? e : e?.message || t("settings.errors.saveFailed");
      console.error(t("settings.errors.saveFailed"), e);
      toast.error(t("settings.toast.saveFailed"), { description: message });
    } finally {
      setSaving(false);
    }
  };

  const handleTestTts = async () => {
    if (!settings) return;
    const text = getTestText(settings.language);
    setTesting(true);
    try {
      await invoke("test_tts_cmd", {
        text,
        language: settings.language,
        quality: settings.quality,
        speed: settings.speed,
        silence: settings.silence,
        voiceStyle: settings.voice_style,
      });
    } catch (e: any) {
      alert(typeof e === "string" ? e : e?.message || t("settings.errors.testFailed"));
    } finally {
      setTesting(false);
    }
  };

  if (!settings) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background p-6">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("settings.title")}</h1>
        <Button variant="outline" size="sm" onClick={onClose}>
          {t("common.back")}
        </Button>
      </div>

      <div className="mx-auto max-w-6xl space-y-6">
        <div className="grid gap-6 lg:grid-cols-2 lg:items-start">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">{t("settings.voice.sectionTitle")}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label>{t("settings.voice.ttsLanguageLabel")}</Label>
                <select
                  className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  value={settings.language}
                  onChange={(e) => update({ language: e.target.value })}
                >
                  {LANGUAGES.map((l) => (
                    <option key={l.code} value={l.code}>
                      {l.name}
                    </option>
                  ))}
                </select>
              </div>

              <div className="space-y-2">
                <Label>{t("settings.voice.voiceStyleLabel")}</Label>
                <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                  {VOICE_STYLE_CODES.map((code) => {
                    const gender = code.startsWith("M")
                      ? t("settings.voiceStyles.male")
                      : t("settings.voiceStyles.female");
                    const selected = settings.voice_style === code;
                    return (
                      <button
                        key={code}
                        onClick={() => update({ voice_style: code })}
                        className={`w-full rounded-md border p-3 text-left transition-colors ${
                          selected
                            ? "border-primary bg-primary/5"
                            : "border-border bg-background hover:bg-muted/50"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${
                            selected
                              ? "bg-primary text-primary-foreground"
                              : "bg-muted text-muted-foreground"
                          }`}>
                            {gender}
                          </span>
                          <span className="font-medium">{code}</span>
                          <span className="text-sm text-muted-foreground">
                            {t(`settings.voiceStyles.${code}.name`)}
                          </span>
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          {t(`settings.voiceStyles.${code}.description`)}
                        </div>
                        <div className="mt-0.5 text-xs text-primary/70">
                          {t("settings.voiceStyles.appliesTo")} {t(`settings.voiceStyles.${code}.useCases`)}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="rounded-md bg-muted p-3">
                <div className="mb-2 text-xs text-muted-foreground">{t("settings.voice.testTextLabel")}</div>
                <div className="text-sm">{getTestText(settings.language)}</div>
                <Button
                  size="sm"
                  className="mt-2 w-full"
                  onClick={handleTestTts}
                  disabled={testing}
                >
                  <Volume2 className="mr-1 h-4 w-4" />
                  {testing ? t("settings.voice.testing") : t("settings.voice.testButton")}
                </Button>
              </div>
            </CardContent>
          </Card>

          <div className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">{t("settings.shortcuts.sectionTitle")}</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label>{t("settings.shortcuts.readLabel")}</Label>
                  <input
                    type="text"
                    value={settings.shortcut}
                    onChange={(e) => update({ shortcut: e.target.value })}
                    placeholder="Ctrl+`"
                    className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                </div>
                <div className="space-y-2">
                  <Label>{t("settings.shortcuts.pauseLabel")}</Label>
                  <input
                    type="text"
                    value={settings.pause_shortcut}
                    onChange={(e) => update({ pause_shortcut: e.target.value })}
                    placeholder={t("settings.shortcuts.pausePlaceholder")}
                    className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                  />
                </div>
                <div className="text-xs text-muted-foreground">
                  {t("settings.shortcuts.hint")}
                </div>
                <Button className="w-full" onClick={handleSave} disabled={saving}>
                  {saving ? t("settings.buttons.saving") : t("settings.buttons.save")}
                </Button>
              </CardContent>
            </Card>
          </div>
        </div>
      </div>
    </div>
  );
}
