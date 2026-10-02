import { useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { formatDate, t } from "../lib/i18n";
import { useSession } from "../lib/session";
import type { OrgOverview } from "../lib/types";
import { Card, ErrorBox, Loading } from "../components/ui";
import { LevelTable, MemberTable } from "../components/OrgView";

export function OrganizationPage() {
  const { me } = useSession();
  const { data: org, error, loading, reload } = useAsync(() => api.get<OrgOverview>("/api/organization"), []);
  const [actionError, setActionError] = useState<unknown>(null);
  if (loading && !org) return <Loading />;
  if (error || !org) return <ErrorBox error={error} />;

  const act = async (fn: () => Promise<unknown>) => {
    setActionError(null);
    try {
      await fn();
      reload();
    } catch (e) {
      setActionError(e);
    }
  };
  const pending = org.members.filter((m) => m.status === "pending");

  return (
    <>
      <h1>{org.name}</h1>
      <ErrorBox error={actionError} />
      <div className="two-col">
        <Card>
          <h2>{t("org.join_code")}</h2>
          <p>{t("org.join_code_hint")}</p>
          <p className="code-display">{org.join_code}</p>
          <button className="button button-secondary" onClick={() => confirm(t("org.rotate_confirm")) && act(() => api.post("/api/organization/join-code"))}>
            {t("org.rotate")}
          </button>
        </Card>
        <Card>
          <h2>{t("org.pending")}</h2>
          {pending.length === 0 && <p className="muted">{t("org.no_pending")}</p>}
          <ul className="plain">
            {pending.map((m) => (
              <li key={m.id} className="pending-row">
                <span>
                  <strong>{m.name}</strong> <span className="muted small">{m.email} · {formatDate(m.requested_at)}</span>
                </span>
                <span className="actions">
                  <button className="button button-small" onClick={() => act(() => api.post(`/api/organization/members/${m.id}/decision`, { approve: true }))}>
                    {t("org.approve")}
                  </button>
                  <button className="button button-small button-secondary" onClick={() => act(() => api.post(`/api/organization/members/${m.id}/decision`, { approve: false }))}>
                    {t("org.reject")}
                  </button>
                </span>
              </li>
            ))}
          </ul>
        </Card>
      </div>
      <h2>{t("org.requirements")}</h2>
      <LevelTable levels={org.levels} current={org.level} />
      <h2>{t("org.members")}</h2>
      <MemberTable
        org={org}
        actions={(m) =>
          m.id === me?.id ? null : (
            <span className="actions">
              <button
                className="button button-small button-ghost"
                onClick={() => act(() => api.post(`/api/organization/members/${m.id}/role`, { org_role: m.org_role === "training_manager" ? "learner" : "training_manager" }))}
              >
                {m.org_role === "training_manager" ? t("org.demote") : t("org.promote")}
              </button>
              <button className="button button-small button-ghost" onClick={() => confirm(t("org.remove_confirm", { name: m.name })) && act(() => api.del(`/api/organization/members/${m.id}`))}>
                {t("org.remove")}
              </button>
            </span>
          )
        }
      />
    </>
  );
}
