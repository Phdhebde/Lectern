import { formatDate, t } from "../lib/i18n";
import type { LevelStatus, OrgOverview } from "../lib/types";
import { Badge, Card, Progress } from "./ui";

export function LevelTable({ levels, current }: { levels: LevelStatus[]; current: string | null }) {
  if (levels.length === 0) return <p className="muted">{t("org.no_levels")}</p>;
  return (
    <div className="levels">
      {levels.map((l) => (
        <Card key={l.slug} className={l.slug === current ? "level-current" : ""}>
          <h3>
            {l.name} {l.met ? <Badge tone="success">{t("org.level_met")}</Badge> : <Badge tone="warning">{t("org.level_not_met")}</Badge>}
            {l.slug === current && <> <Badge tone="primary">{t("org.current_level")}</Badge></>}
          </h3>
          <table className="table compact">
            <thead>
              <tr>
                <th>{t("cert.track")}</th>
                <th>{t("org.valid")}</th>
                <th>{t("org.required")}</th>
                <th>{t("org.missing")}</th>
              </tr>
            </thead>
            <tbody>
              {l.requirements.map((r) => (
                <tr key={r.track_slug}>
                  <td>{r.track_title}</td>
                  <td>{r.valid}</td>
                  <td>{r.required}</td>
                  <td className={r.missing > 0 ? "text-danger" : "text-success"}>{r.missing}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      ))}
    </div>
  );
}

export function MemberTable({ org, actions }: { org: OrgOverview; actions?: (m: OrgOverview["members"][number]) => React.ReactNode }) {
  const members = org.members.filter((m) => m.status === "approved");
  return (
    <table className="table">
      <thead>
        <tr>
          <th>{t("org.member")}</th>
          <th>{t("org.progress")}</th>
          <th>{t("org.certifications")}</th>
          {actions && <th />}
        </tr>
      </thead>
      <tbody>
        {members.map((m) => (
          <tr key={m.id}>
            <td>
              <strong>{m.name}</strong>
              {m.org_role === "training_manager" && <> <Badge tone="primary">{t("roles.training_manager")}</Badge></>}
              <br />
              <span className="muted small">{m.email}</span>
            </td>
            <td>
              {(m.progress ?? []).map((p) => (
                <div key={p.track_slug} className="small">
                  {p.track_title} <Progress value={p.modules_completed} max={p.modules_total} label={p.track_title} />
                </div>
              ))}
            </td>
            <td>
              {(m.certifications ?? []).map((c) => (
                <div key={c.id} className="small">
                  <Badge tone="success">{c.track_title}</Badge> {c.expires_at && <span className="muted">→ {formatDate(c.expires_at)}</span>}
                </div>
              ))}
            </td>
            {actions && <td>{actions(m)}</td>}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
