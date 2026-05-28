import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import zh from "./locales/zh.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import ms from "./locales/ms.json";
import vi from "./locales/vi.json";
import ru from "./locales/ru.json";

export const SUPPORTED_UI_LANGS = ["en", "zh", "ja", "ko", "es", "fr", "ms", "vi", "ru"] as const;
export type UiLang = typeof SUPPORTED_UI_LANGS[number];

export const isUiLang = (v: string): v is UiLang =>
  (SUPPORTED_UI_LANGS as readonly string[]).includes(v);

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
    ja: { translation: ja },
    ko: { translation: ko },
    es: { translation: es },
    fr: { translation: fr },
    ms: { translation: ms },
    vi: { translation: vi },
    ru: { translation: ru },
  },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
