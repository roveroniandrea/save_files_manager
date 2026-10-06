import { en, type MessageKey, type Messages } from "./en";
import { it } from "./it";

export type Locale = "en" | "it";
/** The language setting: a locale, or "system" (the OS language). */
export type LanguageSetting = Locale | "system";

const catalogs: Record<Locale, Messages> = { en, it };

/** The OS/WebView language if supported, otherwise English. */
function systemLocale(): Locale {
  const lang = (typeof navigator === "undefined" ? "" : navigator.language).toLowerCase();
  const base = lang.split("-")[0];
  return base in catalogs ? (base as Locale) : "en";
}

// Until the config is loaded (and if it cannot be), follow the system language.
let locale = $state<Locale>(systemLocale());

export function getLocale(): Locale {
  return locale;
}

export function setLocale(next: Locale) {
  locale = next;
  if (typeof document !== "undefined") document.documentElement.lang = next;
}

/** Applies the language setting ("system" resolves to the OS language). */
export function applyLanguage(setting: LanguageSetting) {
  setLocale(setting === "system" ? systemLocale() : setting);
}

type Params = Record<string, string | number>;

function format(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** Translates `key`, filling `{placeholders}` from `params`. */
export function t(key: MessageKey, params?: Params): string {
  return format(catalogs[locale][key] ?? en[key], params);
}

type PluralBase<K> = K extends `${infer B}_one` ? B : never;
export type PluralKey = PluralBase<MessageKey>;

/** Plural-aware translation: picks `<key>_one` / `<key>_other` for `count`. */
export function tn(key: PluralKey, count: number, params?: Params): string {
  const category = new Intl.PluralRules(locale).select(count) === "one" ? "one" : "other";
  return t(`${key}_${category}` as MessageKey, { count, ...params });
}

/** True if `key` exists in the catalog (for dynamic keys such as error kinds). */
export function hasKey(key: string): key is MessageKey {
  return key in en;
}
