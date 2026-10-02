import { useEffect, useState, type FormEvent } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { api } from "../../lib/api";
import { useAsync } from "../../lib/hooks";
import { t } from "../../lib/i18n";
import type { Annotation } from "../../lib/types";
import { Badge, Card, ErrorBox, Loading } from "../../components/ui";
import { AnnotationEditor } from "../../components/AnnotationEditor";

interface TrackForm {
  title: string;
  summary: string;
  description: string;
  audiences: string[];
  position: number;
  prerequisite: string | null;
  prerequisites: string;
  estimated_minutes: number;
  scenarios_required: boolean;
  validity_months: number | null;
  module_quiz_pass_percent: number;
  published: boolean;
  badge: { label?: string; subtitle?: string; color?: string; accent?: string };
  exam: unknown;
  recert_exam: unknown;
}

interface ModuleForm {
  slug: string;
  position: number;
  title: string;
  video: string | null;
  captions: string | null;
  duration_minutes: number;
  doc_url: string | null;
  body: string;
  attachments: { asset_id: string; label: string; name: string }[];
}

interface StepForm {
  action: string;
  image_asset: string | null;
  alt: string;
  expected: string;
  annotations: Annotation[];
}

interface ScenarioForm {
  slug: string;
  position: number;
  title: string;
  kind: "implementation" | "diagnostic";
  exam_only: boolean;
  family: string | null;
  context: string;
  pitfalls: string;
  steps: StepForm[];
}

interface QuestionForm {
  ref: string;
  pool: "quiz" | "exam" | "recert" | "case";
  module: string | null;
  scenario: string | null;
  format: "choice" | "written";
  prompt: string;
  explanation: string;
  choices: { id?: string; text: string; correct: boolean }[];
  active: boolean;
  answered?: number;
  correct?: number;
}

interface TrackData {
  slug: string;
  track: TrackForm;
  modules: ModuleForm[];
  scenarios: ScenarioForm[];
  questions: QuestionForm[];
}

const NEW_TRACK: TrackForm = {
  title: "",
  summary: "",
  description: "",
  audiences: ["partner"],
  position: 10,
  prerequisite: null,
  prerequisites: "",
  estimated_minutes: 60,
  scenarios_required: false,
  validity_months: 24,
  module_quiz_pass_percent: 70,
  published: false,
  badge: {},
  exam: {
    free_attempts: 2,
    cooldown_days: 7,
    bank_factor: 3,
    sections: [{ title: "QCM", kind: "questions", pool: "exam", question_count: 30, duration_minutes: 40, pass_percent: 75 }],
  },
  recert_exam: null,
};

async function uploadFiles(files: FileList | File[]): Promise<{ asset_id: string; name: string; url: string }[]> {
  const fd = new FormData();
  Array.from(files).forEach((f) => fd.append("file", f, f.name));
  return api.upload("/api/admin/assets", fd);
}

/** Current value of a theme colour token, used as the default of colour pickers. */
function tokenColor(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function Field({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label>
      {label}
      {children}
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}

function TrackSettings({ slug, initial, onSaved }: { slug: string; initial: TrackForm; onSaved: () => void }) {
  const [f, setF] = useState(initial);
  const [exam, setExam] = useState(JSON.stringify(initial.exam, null, 2));
  const [recert, setRecert] = useState(initial.recert_exam ? JSON.stringify(initial.recert_exam, null, 2) : "");
  const [error, setError] = useState<unknown>(null);
  const [ok, setOk] = useState(false);
  const [badgeVersion, setBadgeVersion] = useState(0);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setOk(false);
    try {
      const body = { ...f, exam: JSON.parse(exam), recert_exam: recert.trim() ? JSON.parse(recert) : null };
      await api.put(`/api/admin/tracks/${slug}`, body);
      setOk(true);
      setBadgeVersion((v) => v + 1);
      onSaved();
    } catch (err) {
      setError(err instanceof SyntaxError ? new Error(t("editor.invalid_json")) : err);
    }
  };
  const toggleAudience = (a: string) => setF({ ...f, audiences: f.audiences.includes(a) ? f.audiences.filter((x) => x !== a) : [...f.audiences, a] });
  return (
    <form className="form" onSubmit={save}>
      <div className="two-col">
        <Field label={t("editor.title")}>
          <input value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} required />
        </Field>
        <Field label={t("editor.summary")}>
          <input value={f.summary} onChange={(e) => setF({ ...f, summary: e.target.value })} />
        </Field>
      </div>
      <Field label={t("editor.description")} hint={t("editor.markdown")}>
        <textarea rows={5} value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
      </Field>
      <fieldset>
        <legend>{t("admin.audiences")}</legend>
        {["public", "partner", "customer"].map((a) => (
          <label key={a} className="check-inline">
            <input type="checkbox" checked={f.audiences.includes(a)} onChange={() => toggleAudience(a)} /> {t(`audience.${a}`)}
          </label>
        ))}
      </fieldset>
      <div className="three-col">
        <Field label={t("editor.position")}>
          <input type="number" value={f.position} onChange={(e) => setF({ ...f, position: Number(e.target.value) })} />
        </Field>
        <Field label={t("editor.estimated_minutes")}>
          <input type="number" value={f.estimated_minutes} onChange={(e) => setF({ ...f, estimated_minutes: Number(e.target.value) })} />
        </Field>
        <Field label={t("editor.validity_months")} hint={t("editor.validity_hint")}>
          <input type="number" value={f.validity_months ?? ""} onChange={(e) => setF({ ...f, validity_months: e.target.value ? Number(e.target.value) : null })} />
        </Field>
        <Field label={t("editor.prerequisite")}>
          <input value={f.prerequisite ?? ""} onChange={(e) => setF({ ...f, prerequisite: e.target.value || null })} placeholder="slug" />
        </Field>
        <Field label={t("editor.quiz_pass")}>
          <input type="number" min={0} max={100} value={f.module_quiz_pass_percent} onChange={(e) => setF({ ...f, module_quiz_pass_percent: Number(e.target.value) })} />
        </Field>
      </div>
      <Field label={t("track.prerequisites")} hint={t("editor.markdown")}>
        <textarea rows={2} value={f.prerequisites} onChange={(e) => setF({ ...f, prerequisites: e.target.value })} />
      </Field>
      <label className="check-inline">
        <input type="checkbox" checked={f.scenarios_required} onChange={(e) => setF({ ...f, scenarios_required: e.target.checked })} /> {t("track.scenarios_required")}
      </label>
      <label className="check-inline">
        <input type="checkbox" checked={f.published} onChange={(e) => setF({ ...f, published: e.target.checked })} /> {t("editor.published")}
      </label>
      <fieldset>
        <legend>{t("editor.badge")}</legend>
        <div className="badge-editor">
          <img src={`/ob/badges/${slug}/image.svg?v=${badgeVersion}`} alt="" width={96} height={96} />
          <div className="three-col">
            <Field label={t("editor.badge_label")}>
              <input value={f.badge.label ?? ""} onChange={(e) => setF({ ...f, badge: { ...f.badge, label: e.target.value || undefined } })} />
            </Field>
            <Field label={t("editor.badge_subtitle")}>
              <input value={f.badge.subtitle ?? ""} onChange={(e) => setF({ ...f, badge: { ...f.badge, subtitle: e.target.value || undefined } })} />
            </Field>
            <Field label={t("editor.badge_color")}>
              <input type="color" value={f.badge.color ?? tokenColor("--color-primary")} onChange={(e) => setF({ ...f, badge: { ...f.badge, color: e.target.value } })} />
            </Field>
            <Field label={t("editor.badge_accent")}>
              <input type="color" value={f.badge.accent ?? tokenColor("--color-accent")} onChange={(e) => setF({ ...f, badge: { ...f.badge, accent: e.target.value } })} />
            </Field>
          </div>
        </div>
      </fieldset>
      <Field label={t("editor.exam")} hint={t("editor.exam_hint")}>
        <textarea rows={14} className="mono" value={exam} onChange={(e) => setExam(e.target.value)} />
      </Field>
      <Field label={t("editor.recert_exam")} hint={t("editor.recert_hint")}>
        <textarea rows={6} className="mono" value={recert} onChange={(e) => setRecert(e.target.value)} />
      </Field>
      <ErrorBox error={error} />
      {ok && <div className="alert alert-success">{t("common.saved")}</div>}
      <button className="button">{t("common.save")}</button>
    </form>
  );
}

function ModuleEditor({ slug, initial, onDone }: { slug: string; initial: ModuleForm; onDone: () => void }) {
  const [f, setF] = useState(initial);
  const [error, setError] = useState<unknown>(null);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.put(`/api/admin/tracks/${slug}/modules/${f.slug}`, { ...f, video: f.video || null, captions: f.captions || null, doc_url: f.doc_url || null });
      onDone();
    } catch (err) {
      setError(err);
    }
  };
  return (
    <form className="form" onSubmit={save}>
      <div className="three-col">
        <Field label="slug">
          <input value={f.slug} onChange={(e) => setF({ ...f, slug: e.target.value })} pattern="[a-z0-9][a-z0-9-]*" required disabled={!!initial.slug} />
        </Field>
        <Field label={t("editor.title")}>
          <input value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} required />
        </Field>
        <Field label={t("editor.position")}>
          <input type="number" value={f.position} onChange={(e) => setF({ ...f, position: Number(e.target.value) })} />
        </Field>
        <Field label={t("editor.video")} hint={t("editor.video_hint")}>
          <input value={f.video ?? ""} onChange={(e) => setF({ ...f, video: e.target.value })} />
        </Field>
        <Field label={t("editor.captions")}>
          <input value={f.captions ?? ""} onChange={(e) => setF({ ...f, captions: e.target.value })} placeholder=".vtt" />
        </Field>
        <Field label={t("editor.duration")}>
          <input type="number" value={f.duration_minutes} onChange={(e) => setF({ ...f, duration_minutes: Number(e.target.value) })} />
        </Field>
      </div>
      <Field label={t("editor.doc_url")} hint={t("editor.doc_url_hint")}>
        <input value={f.doc_url ?? ""} onChange={(e) => setF({ ...f, doc_url: e.target.value })} />
      </Field>
      <Field label={t("module.sheet")} hint={t("editor.markdown")}>
        <textarea rows={12} value={f.body} onChange={(e) => setF({ ...f, body: e.target.value })} />
      </Field>
      <fieldset>
        <legend>{t("module.attachments")}</legend>
        {f.attachments.map((a, i) => (
          <div key={a.asset_id} className="inline-form">
            <input value={a.label} onChange={(e) => setF({ ...f, attachments: f.attachments.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)) })} />
            <span className="muted small">{a.name}</span>
            <button type="button" className="button button-small button-ghost" onClick={() => setF({ ...f, attachments: f.attachments.filter((_, j) => j !== i) })}>
              {t("common.delete")}
            </button>
          </div>
        ))}
        <input
          type="file"
          multiple
          onChange={async (e) => {
            if (!e.target.files) return;
            try {
              const up = await uploadFiles(e.target.files);
              setF({ ...f, attachments: [...f.attachments, ...up.map((u) => ({ asset_id: u.asset_id, label: u.name, name: u.name }))] });
            } catch (err) {
              setError(err);
            }
          }}
        />
      </fieldset>
      <ErrorBox error={error} />
      <div className="actions">
        <button className="button">{t("common.save")}</button>
        <button type="button" className="button button-ghost" onClick={onDone}>
          {t("common.cancel")}
        </button>
      </div>
    </form>
  );
}

function ScenarioEditor({ slug, initial, onDone }: { slug: string; initial: ScenarioForm; onDone: () => void }) {
  const [f, setF] = useState(initial);
  const [error, setError] = useState<unknown>(null);
  const setStep = (i: number, s: Partial<StepForm>) => setF({ ...f, steps: f.steps.map((x, j) => (j === i ? { ...x, ...s } : x)) });
  const move = (i: number, d: number) => {
    const steps = [...f.steps];
    const [s] = steps.splice(i, 1);
    steps.splice(i + d, 0, s);
    setF({ ...f, steps });
  };
  const save = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.put(`/api/admin/tracks/${slug}/scenarios/${f.slug}`, { ...f, family: f.family || null });
      onDone();
    } catch (err) {
      setError(err);
    }
  };
  return (
    <form className="form" onSubmit={save}>
      <div className="three-col">
        <Field label="slug">
          <input value={f.slug} onChange={(e) => setF({ ...f, slug: e.target.value })} pattern="[a-z0-9][a-z0-9-]*" required disabled={!!initial.slug} />
        </Field>
        <Field label={t("editor.title")}>
          <input value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} required />
        </Field>
        <Field label={t("editor.kind")}>
          <select value={f.kind} onChange={(e) => setF({ ...f, kind: e.target.value as ScenarioForm["kind"] })}>
            <option value="implementation">{t("scenario.kind.implementation")}</option>
            <option value="diagnostic">{t("scenario.kind.diagnostic")}</option>
          </select>
        </Field>
        <Field label={t("editor.family")} hint={t("editor.family_hint")}>
          <input value={f.family ?? ""} onChange={(e) => setF({ ...f, family: e.target.value })} />
        </Field>
        <Field label={t("editor.position")}>
          <input type="number" value={f.position} onChange={(e) => setF({ ...f, position: Number(e.target.value) })} />
        </Field>
      </div>
      <label className="check-inline">
        <input type="checkbox" checked={f.exam_only} onChange={(e) => setF({ ...f, exam_only: e.target.checked })} /> {t("editor.exam_only")}
      </label>
      <Field label={t("scenario.context")} hint={t("editor.markdown")}>
        <textarea rows={4} value={f.context} onChange={(e) => setF({ ...f, context: e.target.value })} />
      </Field>
      <Field label={t("scenario.pitfalls")} hint={t("editor.markdown")}>
        <textarea rows={3} value={f.pitfalls} onChange={(e) => setF({ ...f, pitfalls: e.target.value })} />
      </Field>
      <div className="alert alert-warning small">{t("editor.fictitious_data")}</div>
      {f.steps.map((s, i) => (
        <Card key={i} className="subtle">
          <div className="page-head">
            <h3>{t("scenario.step_of", { n: i + 1, total: f.steps.length })}</h3>
            <span className="actions">
              <button type="button" className="button button-small button-ghost" disabled={i === 0} onClick={() => move(i, -1)}>
                ↑
              </button>
              <button type="button" className="button button-small button-ghost" disabled={i === f.steps.length - 1} onClick={() => move(i, 1)}>
                ↓
              </button>
              <button type="button" className="button button-small button-ghost" onClick={() => setF({ ...f, steps: f.steps.filter((_, j) => j !== i) })}>
                {t("common.delete")}
              </button>
            </span>
          </div>
          <Field label={t("scenario.action")}>
            <textarea rows={2} value={s.action} onChange={(e) => setStep(i, { action: e.target.value })} required />
          </Field>
          <Field label={t("editor.screenshot")}>
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp"
              onChange={async (e) => {
                if (!e.target.files?.length) return;
                try {
                  const [up] = await uploadFiles(e.target.files);
                  setStep(i, { image_asset: up.asset_id, annotations: [] });
                } catch (err) {
                  setError(err);
                }
              }}
            />
          </Field>
          {s.image_asset && (
            <>
              <Field label={t("editor.alt")}>
                <input value={s.alt} onChange={(e) => setStep(i, { alt: e.target.value })} />
              </Field>
              <AnnotationEditor src={`/api/assets/${s.image_asset}`} value={s.annotations} onChange={(a) => setStep(i, { annotations: a })} />
            </>
          )}
          <Field label={t("scenario.expected")}>
            <textarea rows={2} value={s.expected} onChange={(e) => setStep(i, { expected: e.target.value })} />
          </Field>
        </Card>
      ))}
      <button type="button" className="button button-secondary" onClick={() => setF({ ...f, steps: [...f.steps, { action: "", image_asset: null, alt: "", expected: "", annotations: [] }] })}>
        {t("editor.add_step")}
      </button>
      <ErrorBox error={error} />
      <div className="actions">
        <button className="button">{t("common.save")}</button>
        <button type="button" className="button button-ghost" onClick={onDone}>
          {t("common.cancel")}
        </button>
      </div>
    </form>
  );
}

function QuestionEditor({ slug, initial, modules, scenarios, onDone }: { slug: string; initial: QuestionForm; modules: string[]; scenarios: string[]; onDone: () => void }) {
  const [f, setF] = useState(initial);
  const [error, setError] = useState<unknown>(null);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.put(`/api/admin/questions/${encodeURIComponent(f.ref)}`, { ...f, track: slug });
      onDone();
    } catch (err) {
      setError(err);
    }
  };
  const setChoice = (i: number, c: Partial<QuestionForm["choices"][number]>) => setF({ ...f, choices: f.choices.map((x, j) => (j === i ? { ...x, ...c } : x)) });
  return (
    <form className="form" onSubmit={save}>
      <div className="three-col">
        <Field label="ref">
          <input value={f.ref} onChange={(e) => setF({ ...f, ref: e.target.value })} required disabled={!!initial.ref} />
        </Field>
        <Field label={t("editor.pool")}>
          <select value={f.pool} onChange={(e) => setF({ ...f, pool: e.target.value as QuestionForm["pool"] })}>
            {["quiz", "exam", "recert", "case"].map((p) => (
              <option key={p} value={p}>
                {t(`editor.pools.${p}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("editor.format")}>
          <select value={f.format} onChange={(e) => setF({ ...f, format: e.target.value as QuestionForm["format"], choices: e.target.value === "written" ? [] : f.choices })}>
            <option value="choice">{t("editor.formats.choice")}</option>
            <option value="written">{t("editor.formats.written")}</option>
          </select>
        </Field>
        <Field label={t("editor.module")}>
          <select value={f.module ?? ""} onChange={(e) => setF({ ...f, module: e.target.value || null })}>
            <option value="">—</option>
            {modules.map((m) => (
              <option key={m}>{m}</option>
            ))}
          </select>
        </Field>
        <Field label={t("editor.scenario")}>
          <select value={f.scenario ?? ""} onChange={(e) => setF({ ...f, scenario: e.target.value || null })}>
            <option value="">—</option>
            {scenarios.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </Field>
      </div>
      <Field label={t("editor.prompt")} hint={t("editor.markdown")}>
        <textarea rows={3} value={f.prompt} onChange={(e) => setF({ ...f, prompt: e.target.value })} required />
      </Field>
      {f.format === "choice" && (
        <fieldset>
          <legend>{t("editor.choices")}</legend>
          {f.choices.map((c, i) => (
            <div key={i} className="inline-form">
              <label className="check-inline">
                <input type="checkbox" checked={c.correct} onChange={(e) => setChoice(i, { correct: e.target.checked })} /> {t("editor.correct")}
              </label>
              <input className="grow" value={c.text} onChange={(e) => setChoice(i, { text: e.target.value })} required />
              <button type="button" className="button button-small button-ghost" onClick={() => setF({ ...f, choices: f.choices.filter((_, j) => j !== i) })}>
                {t("common.delete")}
              </button>
            </div>
          ))}
          <button type="button" className="button button-small button-secondary" onClick={() => setF({ ...f, choices: [...f.choices, { text: "", correct: false }] })}>
            {t("editor.add_choice")}
          </button>
        </fieldset>
      )}
      <Field label={t("editor.explanation")} hint={t("editor.explanation_hint")}>
        <textarea rows={2} value={f.explanation} onChange={(e) => setF({ ...f, explanation: e.target.value })} />
      </Field>
      <label className="check-inline">
        <input type="checkbox" checked={f.active} onChange={(e) => setF({ ...f, active: e.target.checked })} /> {t("editor.active")}
      </label>
      <ErrorBox error={error} />
      <div className="actions">
        <button className="button">{t("common.save")}</button>
        <button type="button" className="button button-ghost" onClick={onDone}>
          {t("common.cancel")}
        </button>
      </div>
    </form>
  );
}

type Editing = { kind: "module"; value: ModuleForm } | { kind: "scenario"; value: ScenarioForm } | { kind: "question"; value: QuestionForm } | null;

export function TrackEditor() {
  const { slug = "" } = useParams();
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const isNew = params.get("new") === "1";
  const { data, error, loading, reload } = useAsync(
    () => (isNew ? Promise.resolve<TrackData>({ slug, track: NEW_TRACK, modules: [], scenarios: [], questions: [] }) : api.get<TrackData>(`/api/admin/tracks/${slug}`)),
    [slug, isNew],
  );
  const [editing, setEditing] = useState<Editing>(null);
  const [pool, setPool] = useState<string>("all");
  const [announce, setAnnounce] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => window.scrollTo(0, 0), [editing]);

  if (loading && !data) return <Loading />;
  if (error || !data) return <ErrorBox error={error} />;
  const done = () => {
    setEditing(null);
    reload();
  };
  const questions = data.questions.filter((q) => pool === "all" || q.pool === pool);

  return (
    <>
      <p className="breadcrumb">
        <Link to="/admin">{t("nav.admin")}</Link> › {slug}
      </p>
      <h1>{data.track.title || slug}</h1>
      {msg && <div className="alert alert-success">{msg}</div>}
      {editing?.kind === "module" && <Card><ModuleEditor slug={slug} initial={editing.value} onDone={done} /></Card>}
      {editing?.kind === "scenario" && <Card><ScenarioEditor slug={slug} initial={editing.value} onDone={done} /></Card>}
      {editing?.kind === "question" && (
        <Card>
          <QuestionEditor slug={slug} initial={editing.value} modules={data.modules.map((m) => m.slug)} scenarios={data.scenarios.map((s) => s.slug)} onDone={done} />
        </Card>
      )}
      {!editing && (
        <>
          <Card>
            <h2>{t("editor.settings")}</h2>
            <TrackSettings slug={slug} initial={data.track} onSaved={() => (isNew ? navigate(`/admin/tracks/${slug}`, { replace: true }) : reload())} />
          </Card>
          {!isNew && (
            <>
              <Card>
                <div className="page-head">
                  <h2>{t("track.modules")}</h2>
                  <button
                    className="button button-secondary"
                    onClick={() => setEditing({ kind: "module", value: { slug: "", position: data.modules.length + 1, title: "", video: null, captions: null, duration_minutes: 5, doc_url: null, body: "", attachments: [] } })}
                  >
                    {t("editor.add_module")}
                  </button>
                </div>
                <ol className="module-list">
                  {data.modules.map((m) => (
                    <li key={m.slug}>
                      <button className="link" onClick={() => setEditing({ kind: "module", value: m })}>
                        {m.title}
                      </button>
                      <span className="muted small">{m.slug}</span>
                      <button className="button button-small button-ghost" onClick={() => confirm(t("editor.delete_confirm")) && api.del(`/api/admin/tracks/${slug}/modules/${m.slug}`).then(reload)}>
                        {t("common.delete")}
                      </button>
                    </li>
                  ))}
                </ol>
              </Card>
              <Card>
                <div className="page-head">
                  <h2>{t("track.scenarios")}</h2>
                  <button
                    className="button button-secondary"
                    onClick={() => setEditing({ kind: "scenario", value: { slug: "", position: data.scenarios.length + 1, title: "", kind: "diagnostic", exam_only: false, family: null, context: "", pitfalls: "", steps: [] } })}
                  >
                    {t("editor.add_scenario")}
                  </button>
                </div>
                <ul className="module-list">
                  {data.scenarios.map((s) => (
                    <li key={s.slug}>
                      <button className="link" onClick={() => setEditing({ kind: "scenario", value: s })}>
                        {s.title}
                      </button>
                      {s.exam_only && <Badge tone="warning">{t("editor.exam_only_short")}</Badge>}
                      <span className="muted small">{t("scenario.steps", { n: s.steps.length })}</span>
                      <button className="button button-small button-ghost" onClick={() => confirm(t("editor.delete_confirm")) && api.del(`/api/admin/tracks/${slug}/scenarios/${s.slug}`).then(reload)}>
                        {t("common.delete")}
                      </button>
                    </li>
                  ))}
                </ul>
              </Card>
              <Card>
                <div className="page-head">
                  <h2>{t("editor.questions")}</h2>
                  <span className="actions">
                    <select aria-label={t("editor.pool")} value={pool} onChange={(e) => setPool(e.target.value)}>
                      <option value="all">{t("editor.all")}</option>
                      {["quiz", "exam", "recert", "case"].map((p) => (
                        <option key={p} value={p}>
                          {t(`editor.pools.${p}`)} ({data.questions.filter((q) => q.pool === p && q.active).length})
                        </option>
                      ))}
                    </select>
                    <button
                      className="button button-secondary"
                      onClick={() =>
                        setEditing({
                          kind: "question",
                          value: { ref: `${slug}-${Date.now().toString(36)}`, pool: "exam", module: null, scenario: null, format: "choice", prompt: "", explanation: "", choices: [{ text: "", correct: true }, { text: "", correct: false }], active: true },
                        })
                      }
                    >
                      {t("editor.add_question")}
                    </button>
                  </span>
                </div>
                <table className="table compact">
                  <tbody>
                    {questions.map((q) => (
                      <tr key={q.ref} className={q.active ? "" : "row-muted"}>
                        <td className="mono small">{q.ref}</td>
                        <td>
                          <Badge>{t(`editor.pools.${q.pool}`)}</Badge>
                        </td>
                        <td>
                          <button className="link" onClick={() => setEditing({ kind: "question", value: q })}>
                            {q.prompt.slice(0, 120)}
                          </button>
                        </td>
                        <td className="small">{q.answered ? `${Math.round(((q.correct ?? 0) * 100) / q.answered)} % (${q.answered})` : "—"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Card>
              <Card>
                <h2>{t("editor.announce")}</h2>
                <form
                  className="form"
                  onSubmit={async (e) => {
                    e.preventDefault();
                    const r = await api.post<{ recipients: number }>(`/api/admin/tracks/${slug}/announce`, { message: announce });
                    setMsg(t("editor.announced", { n: r.recipients }));
                    setAnnounce("");
                  }}
                >
                  <textarea rows={3} value={announce} onChange={(e) => setAnnounce(e.target.value)} required placeholder={t("editor.announce_hint")} />
                  <button className="button button-secondary">{t("editor.send")}</button>
                </form>
              </Card>
            </>
          )}
        </>
      )}
    </>
  );
}
