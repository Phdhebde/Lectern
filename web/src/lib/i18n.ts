// Interface texts come from translation files only: no brand name in components.
// Instances override any key with <branding>/locales/<lang>.json (served by /api/i18n/<lang>).
import fr from "../i18n/fr.json";
import en from "../i18n/en.json";

type Dict = { [key: string]: string | Dict };

const bundled: Record<string, Dict> = { fr, en };
let active: Dict = fr;
let vars: Record<string, string> = {};

export function deepMerge(base: Dict, extra: Dict): Dict {
  const out: Dict = { ...base };
  for (const [k, v] of Object.entries(extra)) {
    const current = out[k];
    out[k] = typeof v === "object" && typeof current === "object" ? deepMerge(current, v) : v;
  }
  return out;
}

export async function initI18n(locale: string, globals: Record<string, string>) {
  const lang = bundled[locale] ? locale : "en";
  let overrides: Dict = {};
  try {
    const res = await fetch(`/api/i18n/${lang}`);
    if (res.ok) overrides = await res.json();
  } catch {
    /* offline: bundled texts only */
  }
  active = deepMerge(deepMerge(bundled.en, bundled[lang]), overrides);
  vars = globals;
  document.documentElement.lang = lang;
}

export function lookup(dict: Dict, key: string): string | undefined {
  let cur: string | Dict | undefined = dict;
  for (const part of key.split(".")) {
    if (typeof cur !== "object") return undefined;
    cur = cur[part];
  }
  return typeof cur === "string" ? cur : undefined;
}

export function interpolate(template: string, params: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (m, name) => (name in params ? String(params[name]) : m));
}

/** Translates `key`, replacing `{name}` placeholders with params and instance globals. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const template = lookup(active, key) ?? key;
  return interpolate(template, { ...vars, ...params });
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString(document.documentElement.lang || undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(document.documentElement.lang || undefined);
}
