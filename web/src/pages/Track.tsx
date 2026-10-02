import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { formatDate, t } from "../lib/i18n";
import { useSession } from "../lib/session";
import type { ExamStatus, TrackDetail } from "../lib/types";
import { Badge, Card, ErrorBox, Html, Loading, Progress, minutes } from "../components/ui";

function blockerText(b: string): string {
  const [kind, ...rest] = b.split(":");
  switch (kind) {
    case "modules":
      return t("exam.blocker.modules", { n: rest[0] });
    case "scenarios":
      return t("exam.blocker.scenarios", { n: rest[0] });
    case "prerequisite":
      return t("exam.blocker.prerequisite", { track: rest[0] });
    case "exam_not_ready":
      return t("exam.blocker.not_ready");
    default:
      return b;
  }
}

function ExamPanel({ track, status, onStarted }: { track: TrackDetail; status: ExamStatus; onStarted: (id: string) => void }) {
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const start = async () => {
    if (!confirm(t("exam.confirm_start"))) return;
    setBusy(true);
    try {
      const r = await api.post<{ attempt_id: string }>(`/api/tracks/${track.slug}/exam`);
      onStarted(r.attempt_id);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const elig = status.eligibility;
  const err = elig && "Err" in elig ? elig.Err : null;
  return (
    <Card>
      <h2>{status.purpose === "recertification" ? t("exam.recert_title") : t("exam.title")}</h2>
      <ul className="plain">
        {status.sections.map((s, i) => (
          <li key={i}>
            <strong>{s.title}</strong> — {s.items} · {minutes(s.duration_minutes)}
            {s.pass_percent > 0 && <> · {t("exam.threshold", { n: s.pass_percent })}</>}
          </li>
        ))}
      </ul>
      {status.free_attempts !== null && (
        <p className="muted small">
          {t("exam.attempts_policy", { free: status.free_attempts, used: status.attempts_used, days: track.exam.cooldown_days })}
          {status.credits > 0 && <> {t("exam.credits", { n: status.credits })}</>}
        </p>
      )}
      {status.active_attempt ? (
        <Link className="button" to={`/exam/${status.active_attempt}`}>
          {t("exam.resume")}
        </Link>
      ) : status.purpose === null ? (
        <p>
          {status.recert_opens_at
            ? t("exam.recert_opens", { date: formatDate(status.recert_opens_at) })
            : t("exam.already_certified")}
        </p>
      ) : status.blockers.length > 0 ? (
        <ul className="blockers">
          {status.blockers.map((b) => (
            <li key={b}>{blockerText(b)}</li>
          ))}
        </ul>
      ) : err ? (
        <div className="alert alert-warning">
          {t(`exam.ineligible.${err.reason}`, { date: formatDate(err.retry_at) })}
        </div>
      ) : (
        <button className="button" onClick={start} disabled={busy}>
          {status.purpose === "recertification" ? t("exam.start_recert") : t("exam.start")}
        </button>
      )}
      <ErrorBox error={error} />
    </Card>
  );
}

export function TrackPage() {
  const { slug } = useParams();
  const { me } = useSession();
  const navigate = useNavigate();
  const { data: track, error, loading, reload } = useAsync(() => api.get<TrackDetail>(`/api/tracks/${slug}`), [slug, me?.id]);

  if (loading && !track) return <Loading />;
  if (error || !track) return <ErrorBox error={error} />;

  const done = track.modules.filter((m) => m.completed).length;
  const next = track.modules.find((m) => !m.completed) ?? track.modules[0];
  const rate = async (rating: number) => {
    await api.post(`/api/tracks/${track.slug}/rating`, { rating });
    reload();
  };

  return (
    <div className="track-layout">
      <div>
        <p className="breadcrumb">
          <Link to="/">{t("nav.catalog")}</Link>
        </p>
        <div className="track-header">
          <img src={`/ob/badges/${track.slug}/image.svg`} alt="" width={96} height={96} />
          <div>
            <h1>{track.title}</h1>
            <p className="lead">{track.summary}</p>
            <p className="meta">
              {minutes(track.estimated_minutes)}
              {track.validity_months && <> · {t("track.validity", { n: track.validity_months })}</>}
            </p>
          </div>
        </div>
        <Html html={track.description_html} />
        {track.prerequisites_html.trim() && (
          <Card className="subtle">
            <h2>{t("track.prerequisites")}</h2>
            <Html html={track.prerequisites_html} />
          </Card>
        )}

        <h2>{t("track.modules")}</h2>
        {me && <Progress value={done} max={track.modules.length} label={t("track.progress")} />}
        <ol className="module-list">
          {track.modules.map((m) => (
            <li key={m.id} className={m.completed ? "done" : ""}>
              {me ? <Link to={`/tracks/${track.slug}/modules/${m.slug}`}>{m.title}</Link> : <span>{m.title}</span>}
              <span className="muted small">
                {minutes(m.duration_minutes)}
                {m.quiz_questions > 0 && <> · {t("track.quiz")}</>}
              </span>
              {m.completed && <Badge tone="success">{t("track.completed")}</Badge>}
            </li>
          ))}
        </ol>

        {track.scenarios.length > 0 && (
          <>
            <h2>{t("track.scenarios")}</h2>
            {track.scenarios_required && <p className="muted">{t("track.scenarios_required")}</p>}
            <ul className="module-list">
              {track.scenarios.map((s) => (
                <li key={s.id} className={s.completed ? "done" : ""}>
                  {me ? <Link to={`/tracks/${track.slug}/scenarios/${s.slug}`}>{s.title}</Link> : <span>{s.title}</span>}
                  <span className="muted small">{t(`scenario.kind.${s.kind}`)} · {t("scenario.steps", { n: s.steps })}</span>
                  {s.completed && <Badge tone="success">{t("track.completed")}</Badge>}
                </li>
              ))}
            </ul>
          </>
        )}
      </div>

      <aside>
        {!me ? (
          <Card>
            <p>{t("track.login_to_start")}</p>
            <Link className="button" to={`/login?return_to=/tracks/${track.slug}`}>
              {t("nav.login")}
            </Link>
          </Card>
        ) : (
          <>
            {next && !track.certification && (
              <Card>
                <Link className="button button-block" to={`/tracks/${track.slug}/modules/${next.slug}`}>
                  {done === 0 ? t("track.start") : t("track.continue")}
                </Link>
              </Card>
            )}
            {track.certification && (
              <Card>
                <h2>{t("cert.title")}</h2>
                <p>
                  <Badge tone={track.certification.status === "valid" ? "success" : "muted"}>{t(`cert.status.${track.certification.status}`)}</Badge>
                  {track.certification.provisional && <> <Badge tone="warning">{t("cert.provisional")}</Badge></>}
                </p>
                <p className="small">
                  {t("cert.issued", { date: formatDate(track.certification.issued_at) })}
                  <br />
                  {track.certification.expires_at ? t("cert.expires", { date: formatDate(track.certification.expires_at) }) : t("cert.no_expiry")}
                </p>
                <Link to="/certifications">{t("cert.see_all")}</Link>
              </Card>
            )}
            {track.exam_status && <ExamPanel track={track} status={track.exam_status} onStarted={(id) => navigate(`/exam/${id}`)} />}
            {track.enrollment && (
              <Card>
                <h2>{t("track.rate")}</h2>
                <div className="rating" role="group" aria-label={t("track.rate")}>
                  {[1, 2, 3, 4, 5].map((n) => (
                    <button
                      key={n}
                      className={`star ${track.enrollment && (track.enrollment.rating ?? 0) >= n ? "on" : ""}`}
                      onClick={() => rate(n)}
                      aria-label={t("track.stars", { n })}
                    >
                      ★
                    </button>
                  ))}
                </div>
              </Card>
            )}
          </>
        )}
      </aside>
    </div>
  );
}
