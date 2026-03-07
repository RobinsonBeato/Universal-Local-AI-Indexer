import en from "./i18n.en.js";
import es from "./i18n.es.js";

export const LANG_STORAGE_KEY = "lupa.settings.language";
const DICTS = { en, es };

export function resolveLang(lang) {
  const raw = String(lang || "").toLowerCase();
  return raw === "en" ? "en" : "es";
}

export function t(lang, key, vars = {}) {
  const locale = resolveLang(lang);
  const template = DICTS[locale]?.[key] ?? DICTS.en?.[key] ?? key;
  return String(template).replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? ""));
}

export function collectionLabel(lang, key) {
  return t(lang, `collection_${key}`);
}
