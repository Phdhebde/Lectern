import { useState } from "react";
import { Link } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { formatDate, formatDateTime, t } from "../lib/i18n";
import type { Certification } from "../lib/types";
import { Badge, Card, ErrorBox, Loading } from "../components/ui";

interface AttemptRow {
  id: string;
  track_slug: string;
  track_title: string;
  purpose: string;
  status: string;
  started_at: string;
  finished_at: string | null;
}

function CertCard({ c }: { c: Certification }) {
  const [copied, setCopied] = useState(false);
  const share = `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(c.verify_url)}`;
  const copy = async () => {
    await navigator.clipboard.writeText(c.verify_url);
    setCopied(true);
  };
  const valid = c.status === "valid";
  return (
    <Card className="cert-card">
      <img src={`/ob/badges/${c.track_slug}/image.svg`} alt="" width={110} height={110} />
      <div>
        <h2>{c.track_title}</h2>
        <p>
          <Badge tone={valid ? "success" : "muted"}>{t(`cert.status.${c.status}`)}</Badge>
          {c.provisional && <> <Badge tone="warning">{t("cert.provisional")}</Badge></>}
        </p>
        <p className="small">
          {t("cert.issued", { date: formatDate(c.issued_at) })}
          <br />
          {c.expires_at ? t("cert.expires", { date: formatDate(c.expires_at) }) : t("cert.no_expiry")}
        </p>
        {valid && (
          <div className="actions">
            <button className="button" onClick={() => api.download(`/api/certifications/${c.id}/certificate.pdf`, `certificate-${c.track_slug}.pdf`)}>
              {t("cert.download_pdf")}
            </button>
            <a className="button button-secondary" href={c.linkedin_add_url} target="_blank" rel="noopener noreferrer">
              {t("cert.linkedin_add")}
            </a>
            <a className="button button-secondary" href={share} target="_blank" rel="noopener noreferrer">
              {t("cert.linkedin_share")}
            </a>
            <a className="button button-secondary" href={c.verify_url} target="_blank" rel="noopener noreferrer">
              {t("cert.verify_page")}
            </a>
            <button className="button button-ghost" onClick={copy}>
              {copied ? t("cert.copied") : t("cert.copy_link")}
            </button>
          </div>
        )}
      </div>
    </Card>
  );
}

export function CertificationsPage() {
  const certs = useAsync(() => api.get<Certification[]>("/api/me/certifications"), []);
  const attempts = useAsync(() => api.get<AttemptRow[]>("/api/me/attempts"), []);
  const current = certs.data?.filter((c) => c.status !== "superseded") ?? [];
  return (
    <>
      <h1>{t("cert.page_title")}</h1>
      {certs.loading && <Loading />}
      <ErrorBox error={certs.error} />
      {certs.data && current.length === 0 && (
        <p className="muted">
          {t("cert.none")} <Link to="/">{t("nav.catalog")}</Link>
        </p>
      )}
      {current.map((c) => (
        <CertCard key={c.id} c={c} />
      ))}
      {attempts.data && attempts.data.length > 0 && (
        <>
          <h2>{t("cert.attempts")}</h2>
          <table className="table">
            <thead>
              <tr>
                <th>{t("cert.track")}</th>
                <th>{t("cert.date")}</th>
                <th>{t("cert.result")}</th>
              </tr>
            </thead>
            <tbody>
              {attempts.data.map((a) => (
                <tr key={a.id}>
                  <td>
                    {a.track_title}
                    {a.purpose === "recertification" && <span className="muted"> ({t("exam.recert_short")})</span>}
                  </td>
                  <td>{formatDateTime(a.started_at)}</td>
                  <td>
                    <Link to={`/exam/${a.id}`}>{t(`exam.status.${a.status}`)}</Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}
