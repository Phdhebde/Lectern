import { Link } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { formatDate, t } from "../lib/i18n";
import { useSession } from "../lib/session";
import type { CatalogEntry } from "../lib/types";
import { Badge, ErrorBox, Loading, Progress, minutes } from "../components/ui";

export function Catalog() {
  const { me, instance } = useSession();
  const { data, error, loading } = useAsync(() => api.get<CatalogEntry[]>("/api/catalog"), [me?.id]);

  return (
    <>
      <section className="hero">
        <h1>{t("catalog.title", { instance: instance.name })}</h1>
        <p className="lead">{t("catalog.lead")}</p>
        {me?.membership?.status === "pending" && <div className="alert alert-warning">{t("catalog.membership_pending", { org: me.membership.org_name })}</div>}
        {me && !me.membership && <p className="muted">{t("catalog.join_hint")} <Link to="/profile">{t("nav.profile")}</Link></p>}
      </section>
      {loading && <Loading />}
      <ErrorBox error={error} />
      <div className="grid">
        {data?.map((track) => (
          <Link key={track.slug} to={`/tracks/${track.slug}`} className="card track-card">
            <img className="track-badge" src={`/ob/badges/${track.slug}/image.svg`} alt="" width={72} height={72} />
            <div>
              <h2>{track.title}</h2>
              <p>{track.summary}</p>
              <p className="meta">
                {t("catalog.modules", { n: track.module_count })} · {minutes(track.estimated_minutes)}
                {track.audiences.includes("public") && <> · <Badge tone="primary">{t("catalog.free")}</Badge></>}
              </p>
              {track.certification ? (
                <p>
                  <Badge tone={track.certification.status === "valid" ? "success" : "muted"}>{t(`cert.status.${track.certification.status}`)}</Badge>{" "}
                  {track.certification.expires_at && <span className="muted small">{t("cert.until", { date: formatDate(track.certification.expires_at) })}</span>}
                </p>
              ) : (
                track.enrolled && <Progress value={track.completed_modules} max={track.module_count} label={t("track.progress")} />
              )}
            </div>
          </Link>
        ))}
      </div>
      {data && data.length === 0 && <p className="muted">{t("catalog.empty")}</p>}
    </>
  );
}
