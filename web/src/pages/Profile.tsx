import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router";
import { api } from "../lib/api";
import { t } from "../lib/i18n";
import { useSession } from "../lib/session";
import { Badge, Card, ErrorBox } from "../components/ui";

export function ProfilePage() {
  const { me, refresh, instance } = useSession();
  const navigate = useNavigate();
  const [name, setName] = useState(me?.display_name ?? "");
  const [code, setCode] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  if (!me) return null;

  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    setSaved(false);
    try {
      await fn();
      await refresh();
      setSaved(true);
    } catch (e) {
      setError(e);
    }
  };

  const saveName = (e: FormEvent) => {
    e.preventDefault();
    run(() => api.patch("/api/me", { display_name: name }));
  };

  const logout = async () => {
    await api.post("/api/auth/logout");
    await refresh();
    navigate("/");
  };

  const deleteAccount = async () => {
    if (!confirm(t("profile.delete_confirm"))) return;
    await api.del("/api/me");
    await refresh();
    navigate("/");
  };

  const m = me.membership;
  return (
    <>
      <h1>{t("profile.title")}</h1>
      <ErrorBox error={error} />
      {saved && <div className="alert alert-success" role="status">{t("common.saved")}</div>}
      <div className="two-col">
        <Card>
          <h2>{t("profile.identity")}</h2>
          <p className="muted">{me.email}</p>
          <form onSubmit={saveName} className="form">
            <label>
              {t("profile.name")}
              <input value={name} onChange={(e) => setName(e.target.value)} required maxLength={120} />
            </label>
            <p className="hint">{t("profile.name_hint")}</p>
            <button className="button">{t("common.save")}</button>
          </form>
          {me.roles.length > 0 && (
            <p>
              {me.roles.map((r) => (
                <Badge key={r} tone="primary">{t(`roles.${r}`)}</Badge>
              ))}{" "}
              {me.mfa ? <Badge tone="success">MFA</Badge> : <span className="muted small">{t("profile.no_mfa")}</span>}
            </p>
          )}
        </Card>

        <Card>
          <h2>{t("profile.organization")}</h2>
          {m ? (
            <>
              <p>
                <strong>{m.org_name}</strong> <Badge tone={m.status === "approved" ? "success" : m.status === "pending" ? "warning" : "danger"}>{t(`membership.${m.status}`)}</Badge>
                {m.org_role === "training_manager" && <> <Badge tone="primary">{t("roles.training_manager")}</Badge></>}
              </p>
              <button className="button button-secondary" onClick={() => confirm(t("profile.leave_confirm")) && run(() => api.del("/api/me/organization"))}>
                {t("profile.leave")}
              </button>
            </>
          ) : (
            <form
              className="form"
              onSubmit={(e) => {
                e.preventDefault();
                run(() => api.post("/api/me/organization", { join_code: code }));
              }}
            >
              <p className="muted">{t("profile.individual")}</p>
              <label>
                {t("profile.join_code")}
                <input value={code} onChange={(e) => setCode(e.target.value)} required autoComplete="off" />
              </label>
              <button className="button">{t("profile.join")}</button>
            </form>
          )}
        </Card>

        <Card>
          <h2>{t("profile.privacy")}</h2>
          <label className="switch">
            <input type="checkbox" checked={me.public_profile} onChange={(e) => run(() => api.patch("/api/me", { public_profile: e.target.checked }))} />
            {t("profile.public_profile")}
          </label>
          <p className="hint">{t("profile.public_profile_hint")}</p>
          <div className="actions">
            <button className="button button-secondary" onClick={() => api.download("/api/me/export", "my-data.json")}>
              {t("profile.export")}
            </button>
            {instance.privacy_policy_url && (
              <a className="button button-ghost" href={instance.privacy_policy_url}>
                {t("footer.privacy")}
              </a>
            )}
          </div>
          <hr />
          <button className="button button-danger" onClick={deleteAccount}>
            {t("profile.delete")}
          </button>
        </Card>
      </div>
      <p>
        <button className="button button-secondary" onClick={logout}>
          {t("nav.logout")}
        </button>
      </p>
    </>
  );
}
