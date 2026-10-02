import { Link, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { t } from "../lib/i18n";
import type { OrgOverview } from "../lib/types";
import { Badge, ErrorBox, Loading } from "../components/ui";
import { LevelTable, MemberTable } from "../components/OrgView";

interface OrgRow {
  id: string;
  name: string;
  kind: string;
  level: string | null;
  members: number;
  pending: number;
  valid_certifications: Record<string, number>;
  level_met: boolean | null;
  highest_level_met: string | null;
}

export function PartnersPage() {
  const { data, error, loading } = useAsync(() => api.get<OrgRow[]>("/api/partners"), []);
  const tracks = Array.from(new Set((data ?? []).flatMap((o) => Object.keys(o.valid_certifications)))).sort();
  return (
    <>
      <div className="page-head">
        <h1>{t("partners.title")}</h1>
        <button className="button button-secondary" onClick={() => api.download("/api/partners/export.csv", "certified.csv")}>
          {t("partners.export")}
        </button>
      </div>
      {loading && <Loading />}
      <ErrorBox error={error} />
      {data && (
        <table className="table">
          <thead>
            <tr>
              <th>{t("partners.organization")}</th>
              <th>{t("partners.level")}</th>
              <th>{t("partners.members")}</th>
              {tracks.map((tr) => (
                <th key={tr}>{tr}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.map((o) => (
              <tr key={o.id}>
                <td>
                  <Link to={`/partners/${o.id}`}>{o.name}</Link> <span className="muted small">{t(`org.kind.${o.kind}`)}</span>
                </td>
                <td>
                  {o.level ?? "—"} {o.level_met === true && <Badge tone="success">✓</Badge>}
                  {o.level_met === false && <Badge tone="danger">{t("org.level_not_met")}</Badge>}
                  {o.highest_level_met && <div className="muted small">{t("partners.highest", { level: o.highest_level_met })}</div>}
                </td>
                <td>
                  {o.members}
                  {o.pending > 0 && <span className="muted"> (+{o.pending})</span>}
                </td>
                {tracks.map((tr) => (
                  <td key={tr}>{o.valid_certifications[tr] ?? 0}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}

export function PartnerDetail() {
  const { id } = useParams();
  const { data: org, error, loading } = useAsync(() => api.get<OrgOverview>(`/api/partners/${id}`), [id]);
  if (loading && !org) return <Loading />;
  if (error || !org) return <ErrorBox error={error} />;
  return (
    <>
      <p className="breadcrumb">
        <Link to="/partners">{t("partners.title")}</Link>
      </p>
      <h1>{org.name}</h1>
      <LevelTable levels={org.levels} current={org.level} />
      <h2>{t("org.members")}</h2>
      <MemberTable org={org} />
    </>
  );
}
