import { useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router";
import { api } from "../../lib/api";
import { useAsync } from "../../lib/hooks";
import { formatDateTime, t } from "../../lib/i18n";
import { useSession } from "../../lib/session";
import { Badge, Card, ErrorBox, Loading } from "../../components/ui";

const TABS = ["content", "organizations", "users", "levels", "stats", "tokens", "audit", "settings"] as const;
type Tab = (typeof TABS)[number];
const AUTHOR_TABS: Tab[] = ["content", "stats"];

function useAction() {
  const [error, setError] = useState<unknown>(null);
  const [message, setMessage] = useState<string | null>(null);
  const run = async (fn: () => Promise<unknown>, success?: string) => {
    setError(null);
    setMessage(null);
    try {
      await fn();
      if (success) setMessage(success);
      return true;
    } catch (e) {
      setError(e);
      return false;
    }
  };
  const feedback = (
    <>
      <ErrorBox error={error} />
      {message && <div className="alert alert-success" role="status">{message}</div>}
    </>
  );
  return { run, feedback };
}

export function AdminPage() {
  const { hasRole, me } = useSession();
  const [params, setParams] = useSearchParams();
  const isAdmin = hasRole("admin");
  const tabs = isAdmin ? TABS : AUTHOR_TABS;
  const tab = (params.get("tab") as Tab) ?? "content";
  const needsMfa = me && !me.mfa && me.mfa_required_for.some((r) => me.roles.includes(r as never));
  return (
    <>
      <h1>{t("nav.admin")}</h1>
      {needsMfa && <div className="alert alert-warning">{t("errors.mfa_required")}</div>}
      <div className="tabs" role="tablist">
        {tabs.map((tb) => (
          <button key={tb} role="tab" aria-selected={tab === tb} className={tab === tb ? "active" : ""} onClick={() => setParams({ tab: tb })}>
            {t(`admin.tabs.${tb}`)}
          </button>
        ))}
      </div>
      {tab === "content" && <ContentTab isAdmin={isAdmin} />}
      {tab === "organizations" && isAdmin && <OrganizationsTab />}
      {tab === "users" && isAdmin && <UsersTab />}
      {tab === "levels" && isAdmin && <LevelsTab />}
      {tab === "stats" && <StatsTab />}
      {tab === "tokens" && isAdmin && <TokensTab />}
      {tab === "audit" && isAdmin && <AuditTab />}
      {tab === "settings" && isAdmin && <SettingsTab />}
    </>
  );
}

interface TrackRow {
  slug: string;
  title: string;
  published: boolean;
  audiences: string[];
  modules: number;
  scenarios: number;
  questions: number;
  readiness: string[];
}

function ContentTab({ isAdmin }: { isAdmin: boolean }) {
  const { data, error, loading, reload } = useAsync(() => api.get<TrackRow[]>("/api/admin/tracks"), []);
  const { run, feedback } = useAction();
  const [newSlug, setNewSlug] = useState("");

  const importPack = async (file: File) => {
    const ok = await run(async () => {
      const r = await api.upload<Record<string, number>>("/api/admin/pack", file);
      return r;
    }, t("admin.pack_imported"));
    if (ok) reload();
  };

  return (
    <>
      {feedback}
      {isAdmin && (
        <Card>
          <h2>{t("admin.pack")}</h2>
          <p className="muted">{t("admin.pack_hint")}</p>
          <div className="actions">
            <label className="button button-secondary">
              {t("admin.pack_import")}
              <input type="file" accept=".zip" hidden onChange={(e) => e.target.files?.[0] && importPack(e.target.files[0])} />
            </label>
            <button className="button button-secondary" onClick={() => api.download("/api/admin/pack", "content-pack.zip")}>
              {t("admin.pack_export")}
            </button>
          </div>
        </Card>
      )}
      {loading && <Loading />}
      <ErrorBox error={error} />
      <table className="table">
        <thead>
          <tr>
            <th>{t("admin.track")}</th>
            <th>{t("admin.audiences")}</th>
            <th>{t("admin.content")}</th>
            <th>{t("admin.readiness")}</th>
          </tr>
        </thead>
        <tbody>
          {data?.map((tr) => (
            <tr key={tr.slug}>
              <td>
                <Link to={`/admin/tracks/${tr.slug}`}>{tr.title}</Link> {!tr.published && <Badge>{t("admin.draft")}</Badge>}
                <div className="muted small">{tr.slug}</div>
              </td>
              <td>{tr.audiences.map((a) => t(`audience.${a}`)).join(", ")}</td>
              <td className="small">
                {t("admin.counts", { m: tr.modules, s: tr.scenarios, q: tr.questions })}
              </td>
              <td>
                {tr.readiness.length === 0 ? (
                  <Badge tone="success">{t("admin.ready")}</Badge>
                ) : (
                  tr.readiness.map((r) => (
                    <div key={r} className="small text-danger">
                      {t("admin.not_ready", { detail: r.split(":").slice(1).join(" ") })}
                    </div>
                  ))
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          location.assign(`/admin/tracks/${newSlug}?new=1`);
        }}
      >
        <input placeholder={t("admin.new_track_slug")} pattern="[a-z0-9][a-z0-9-]*" value={newSlug} onChange={(e) => setNewSlug(e.target.value)} required />
        <button className="button button-secondary">{t("admin.new_track")}</button>
      </form>
    </>
  );
}

interface OrgRow {
  id: string;
  name: string;
  kind: string;
  level: string | null;
  members: number;
  pending: number;
}

function OrganizationsTab() {
  const { data, reload } = useAsync(() => api.get<OrgRow[]>("/api/partners"), []);
  const levels = useAsync(() => api.get<{ slug: string; name: string; org_kind: string }[]>("/api/admin/levels"), []);
  const { run, feedback } = useAction();
  const [form, setForm] = useState({ name: "", kind: "partner", level_slug: "" });
  const [managers, setManagers] = useState<Record<string, string>>({});

  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (await run(() => api.post("/api/admin/organizations", { ...form, level_slug: form.level_slug || null }), t("common.saved"))) {
      setForm({ name: "", kind: "partner", level_slug: "" });
      reload();
    }
  };

  return (
    <>
      {feedback}
      <Card>
        <h2>{t("admin.new_org")}</h2>
        <form className="inline-form" onSubmit={create}>
          <input placeholder={t("admin.org_name")} value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required />
          <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value })} aria-label={t("admin.org_kind")}>
            <option value="partner">{t("org.kind.partner")}</option>
            <option value="customer">{t("org.kind.customer")}</option>
          </select>
          <button className="button">{t("common.create")}</button>
        </form>
      </Card>
      <table className="table">
        <thead>
          <tr>
            <th>{t("partners.organization")}</th>
            <th>{t("partners.level")}</th>
            <th>{t("admin.add_manager")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {data?.map((o) => (
            <tr key={o.id}>
              <td>
                <Link to={`/partners/${o.id}`}>{o.name}</Link> <span className="muted small">{t(`org.kind.${o.kind}`)} · {o.members}</span>
              </td>
              <td>
                <select
                  aria-label={t("partners.level")}
                  value={o.level ?? ""}
                  onChange={(e) => run(() => api.put(`/api/admin/organizations/${o.id}`, { name: o.name, kind: o.kind, level_slug: e.target.value || null })).then(reload)}
                >
                  <option value="">—</option>
                  {levels.data?.filter((l) => l.org_kind === o.kind).map((l) => (
                    <option key={l.slug} value={l.slug}>
                      {l.name}
                    </option>
                  ))}
                </select>
              </td>
              <td>
                <form
                  className="inline-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    run(() => api.post(`/api/admin/organizations/${o.id}/managers`, { email: managers[o.id] }), t("common.saved")).then(reload);
                  }}
                >
                  <input type="email" placeholder="email" value={managers[o.id] ?? ""} onChange={(e) => setManagers({ ...managers, [o.id]: e.target.value })} required />
                  <button className="button button-small">{t("common.add")}</button>
                </form>
              </td>
              <td>
                <button className="button button-small button-ghost" onClick={() => confirm(t("admin.delete_org_confirm", { name: o.name })) && run(() => api.del(`/api/admin/organizations/${o.id}`)).then(reload)}>
                  {t("common.delete")}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

interface UserRow {
  id: string;
  email: string;
  display_name: string;
  created_at: string;
  last_login_at: string | null;
  roles: string[];
  organization: string | null;
  membership_status: string | null;
}

function UsersTab() {
  const [q, setQ] = useState("");
  const { data, reload } = useAsync(() => api.get<UserRow[]>(`/api/admin/users?q=${encodeURIComponent(q)}`), [q]);
  const tracks = useAsync(() => api.get<{ slug: string; title: string }[]>("/api/admin/tracks"), []);
  const { run, feedback } = useAction();
  const [credit, setCredit] = useState<Record<string, string>>({});
  return (
    <>
      {feedback}
      <input className="search" type="search" placeholder={t("admin.search_users")} value={q} onChange={(e) => setQ(e.target.value)} />
      <table className="table">
        <thead>
          <tr>
            <th>{t("org.member")}</th>
            <th>{t("partners.organization")}</th>
            <th>{t("admin.roles")}</th>
            <th>{t("admin.extra_attempt")}</th>
          </tr>
        </thead>
        <tbody>
          {data?.map((u) => (
            <tr key={u.id}>
              <td>
                <strong>{u.display_name}</strong>
                <div className="muted small">
                  {u.email} · {formatDateTime(u.last_login_at)}
                </div>
              </td>
              <td>
                {u.organization ?? "—"} {u.membership_status && <span className="muted small">({t(`membership.${u.membership_status}`)})</span>}
              </td>
              <td>
                {(["admin", "trainer", "channel_manager"] as const).map((r) => (
                  <label key={r} className="check-inline">
                    <input type="checkbox" checked={u.roles.includes(r)} onChange={(e) => run(() => api.post(`/api/admin/users/${u.id}/roles`, { role: r, grant: e.target.checked })).then(reload)} />
                    {t(`roles.${r}`)}
                  </label>
                ))}
              </td>
              <td>
                <form
                  className="inline-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    run(() => api.post(`/api/admin/users/${u.id}/credits`, { track_slug: credit[u.id] }), t("admin.credit_granted"));
                  }}
                >
                  <select aria-label={t("admin.track")} value={credit[u.id] ?? ""} onChange={(e) => setCredit({ ...credit, [u.id]: e.target.value })} required>
                    <option value="">—</option>
                    {tracks.data?.map((tr) => (
                      <option key={tr.slug} value={tr.slug}>
                        {tr.title}
                      </option>
                    ))}
                  </select>
                  <button className="button button-small">{t("admin.grant")}</button>
                </form>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

interface LevelRow {
  slug: string;
  org_kind: string;
  name: string;
  rank: number;
  requirements: Record<string, number>;
}

function LevelsTab() {
  const { data, reload } = useAsync(() => api.get<LevelRow[]>("/api/admin/levels"), []);
  const { run, feedback } = useAction();
  const [draft, setDraft] = useState({ slug: "", org_kind: "partner", name: "", rank: 1, requirements: "{}" });
  const save = async (e: FormEvent) => {
    e.preventDefault();
    let req: Record<string, number>;
    try {
      req = JSON.parse(draft.requirements);
    } catch {
      return run(() => Promise.reject(new Error("JSON")));
    }
    if (await run(() => api.put(`/api/admin/levels/${draft.slug}`, { ...draft, requirements: req }), t("common.saved"))) reload();
  };
  return (
    <>
      {feedback}
      <p className="muted">{t("admin.levels_hint")}</p>
      <table className="table">
        <thead>
          <tr>
            <th>{t("admin.level")}</th>
            <th>{t("admin.org_kind")}</th>
            <th>{t("org.requirements")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {data?.map((l) => (
            <tr key={l.slug}>
              <td>
                {l.rank}. {l.name} <span className="muted small">{l.slug}</span>
              </td>
              <td>{t(`org.kind.${l.org_kind}`)}</td>
              <td className="small">
                {Object.entries(l.requirements)
                  .map(([k, v]) => `${k}: ${v}`)
                  .join(", ")}
              </td>
              <td className="actions">
                <button className="button button-small button-ghost" onClick={() => setDraft({ ...l, requirements: JSON.stringify(l.requirements) })}>
                  {t("common.edit")}
                </button>
                <button className="button button-small button-ghost" onClick={() => run(() => api.del(`/api/admin/levels/${l.slug}`)).then(reload)}>
                  {t("common.delete")}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <Card>
        <form className="form" onSubmit={save}>
          <div className="inline-form">
            <input placeholder="slug" pattern="[a-z0-9][a-z0-9-]*" value={draft.slug} onChange={(e) => setDraft({ ...draft, slug: e.target.value })} required />
            <input placeholder={t("admin.level")} value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} required />
            <select aria-label={t("admin.org_kind")} value={draft.org_kind} onChange={(e) => setDraft({ ...draft, org_kind: e.target.value })}>
              <option value="partner">{t("org.kind.partner")}</option>
              <option value="customer">{t("org.kind.customer")}</option>
            </select>
            <input type="number" aria-label={t("admin.rank")} value={draft.rank} onChange={(e) => setDraft({ ...draft, rank: Number(e.target.value) })} />
          </div>
          <label>
            {t("admin.requirements_json")}
            <input value={draft.requirements} onChange={(e) => setDraft({ ...draft, requirements: e.target.value })} className="mono" />
          </label>
          <button className="button">{t("common.save")}</button>
        </form>
      </Card>
    </>
  );
}

interface Stats {
  users: number;
  signups: { month: string; users: number }[] | null;
  tracks: { slug: string; title: string; enrollments: number; completions: number; average_rating: number | null; attempts: number; passed: number; valid_certifications: number }[] | null;
  questions: { ref: string; track: string; pool: string; prompt: string; answered: number; correct: number; success_rate: number | null; flagged: boolean }[] | null;
}

function StatsTab() {
  const { data, loading, error } = useAsync(() => api.get<Stats>("/api/admin/stats"), []);
  if (loading) return <Loading />;
  if (error || !data) return <ErrorBox error={error} />;
  const pct = (a: number, b: number) => (b ? `${Math.round((100 * a) / b)} %` : "—");
  return (
    <>
      <p>{t("admin.stats_users", { n: data.users })}</p>
      <table className="table">
        <thead>
          <tr>
            <th>{t("admin.track")}</th>
            <th>{t("admin.enrollments")}</th>
            <th>{t("admin.completion")}</th>
            <th>{t("admin.pass_rate")}</th>
            <th>{t("admin.valid_certs")}</th>
            <th>{t("admin.rating")}</th>
          </tr>
        </thead>
        <tbody>
          {data.tracks?.map((tr) => (
            <tr key={tr.slug}>
              <td>{tr.title}</td>
              <td>{tr.enrollments}</td>
              <td>{pct(tr.completions, tr.enrollments)}</td>
              <td>
                {pct(tr.passed, tr.attempts)} <span className="muted small">({tr.attempts})</span>
              </td>
              <td>{tr.valid_certifications}</td>
              <td>{tr.average_rating ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h2>{t("admin.question_stats")}</h2>
      <p className="muted">{t("admin.question_stats_hint")}</p>
      <table className="table compact">
        <thead>
          <tr>
            <th>ref</th>
            <th>{t("admin.question")}</th>
            <th>{t("admin.answered")}</th>
            <th>{t("admin.success")}</th>
          </tr>
        </thead>
        <tbody>
          {data.questions?.map((q) => (
            <tr key={q.ref} className={q.flagged ? "row-flagged" : ""}>
              <td className="mono small">{q.ref}</td>
              <td className="small">{q.prompt}</td>
              <td>{q.answered}</td>
              <td>
                {q.success_rate ?? "—"} % {q.flagged && <Badge tone="danger">{t("admin.to_review")}</Badge>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

function TokensTab() {
  const { data, reload } = useAsync(() => api.get<{ id: string; name: string; created_at: string; last_used_at: string | null; revoked_at: string | null }[]>("/api/admin/api-tokens"), []);
  const { run, feedback } = useAction();
  const [name, setName] = useState("");
  const [created, setCreated] = useState<string | null>(null);
  return (
    <>
      {feedback}
      <p className="muted">{t("admin.tokens_hint")}</p>
      {created && (
        <div className="alert alert-warning">
          {t("admin.token_once")} <code className="mono">{created}</code>
        </div>
      )}
      <form
        className="inline-form"
        onSubmit={async (e) => {
          e.preventDefault();
          await run(async () => setCreated((await api.post<{ token: string }>("/api/admin/api-tokens", { name })).token));
          reload();
        }}
      >
        <input placeholder={t("admin.token_name")} value={name} onChange={(e) => setName(e.target.value)} required />
        <button className="button">{t("common.create")}</button>
      </form>
      <table className="table">
        <tbody>
          {data?.map((tk) => (
            <tr key={tk.id}>
              <td>{tk.name}</td>
              <td className="small">{formatDateTime(tk.created_at)}</td>
              <td className="small">{t("admin.last_used", { date: formatDateTime(tk.last_used_at) })}</td>
              <td>
                {tk.revoked_at ? (
                  <Badge>{t("admin.revoked")}</Badge>
                ) : (
                  <button className="button button-small button-ghost" onClick={() => run(() => api.del(`/api/admin/api-tokens/${tk.id}`)).then(reload)}>
                    {t("admin.revoke")}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="small muted">
        <code>GET /api/v1/certified</code> · <code>Authorization: Bearer …</code>
      </p>
    </>
  );
}

function AuditTab() {
  const { data } = useAsync(() => api.get<{ id: number; at: string; action: string; target: string | null; details: unknown; actor: string | null }[]>("/api/admin/audit"), []);
  return (
    <table className="table compact">
      <tbody>
        {data?.map((a) => (
          <tr key={a.id}>
            <td className="small">{formatDateTime(a.at)}</td>
            <td>{a.actor ?? "—"}</td>
            <td className="mono small">{a.action}</td>
            <td className="mono small">{a.target}</td>
            <td className="mono small">{JSON.stringify(a.details)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function SettingsTab() {
  const { instance } = useSession();
  const { run, feedback } = useAction();
  const [version, setVersion] = useState("");
  const [revokeId, setRevokeId] = useState("");
  const [reason, setReason] = useState("");
  return (
    <>
      {feedback}
      <Card>
        <h2>{t("admin.major_version")}</h2>
        <p>{t("admin.major_version_hint", { current: instance.product_major_version ?? "—" })}</p>
        <form
          className="inline-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (confirm(t("admin.major_version_confirm", { v: version }))) run(() => api.post("/api/admin/product-version", { version }), t("common.saved"));
          }}
        >
          <input value={version} onChange={(e) => setVersion(e.target.value)} required placeholder="3" />
          <button className="button button-danger">{t("admin.declare")}</button>
        </form>
      </Card>
      <Card>
        <h2>{t("admin.revoke_cert")}</h2>
        <form
          className="inline-form"
          onSubmit={(e) => {
            e.preventDefault();
            run(() => api.post(`/api/admin/certifications/${revokeId}/revoke`, { reason }), t("common.saved"));
          }}
        >
          <input placeholder={t("admin.cert_id")} value={revokeId} onChange={(e) => setRevokeId(e.target.value)} required className="mono" />
          <input placeholder={t("admin.reason")} value={reason} onChange={(e) => setReason(e.target.value)} required />
          <button className="button button-danger">{t("admin.revoke")}</button>
        </form>
      </Card>
    </>
  );
}
