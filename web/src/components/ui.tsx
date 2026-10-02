import type { ReactNode } from "react";
import { ApiError } from "../lib/api";
import { t } from "../lib/i18n";

export function Loading() {
  return (
    <p className="muted" role="status">
      {t("common.loading")}
    </p>
  );
}

export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const key = `errors.${error.code}`;
    const text = t(key);
    return text === key ? error.message : text;
  }
  return t("errors.generic");
}

export function ErrorBox({ error }: { error: unknown }) {
  if (!error) return null;
  return (
    <div className="alert alert-danger" role="alert">
      {errorMessage(error)}
    </div>
  );
}

/** Server-rendered HTML, already sanitized by the server (Markdown → ammonia). */
export function Html({ html, className }: { html: string; className?: string }) {
  return <div className={className ?? "prose"} dangerouslySetInnerHTML={{ __html: html }} />;
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <section className={`card ${className ?? ""}`}>{children}</section>;
}

export function Badge({ children, tone }: { children: ReactNode; tone?: "success" | "danger" | "warning" | "muted" | "primary" }) {
  return <span className={`pill pill-${tone ?? "muted"}`}>{children}</span>;
}

export function Progress({ value, max, label }: { value: number; max: number; label?: string }) {
  const pct = max > 0 ? Math.round((value / max) * 100) : 0;
  return (
    <div className="progress" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
      <div className="progress-bar" style={{ width: `${pct}%` }} />
    </div>
  );
}

export function minutes(n: number): string {
  if (n < 60) return t("common.minutes", { n });
  const h = Math.floor(n / 60);
  const m = n % 60;
  return m ? t("common.hours_minutes", { h, m }) : t("common.hours", { h });
}
