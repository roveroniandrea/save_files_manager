import { en, type MessageKey, type Messages } from "./en";

export type Locale = "en";

const catalogs: Record<Locale, Messages> = { en };

let locale = $state<Locale>("en");

export function getLocale(): Locale {
  return locale;
}

export function setLocale(next: Locale) {
  locale = next;
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
