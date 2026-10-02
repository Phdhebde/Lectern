import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { t } from "../lib/i18n";
import type { Answer, AttemptView } from "../lib/types";
import { Badge, Card, ErrorBox, Html, Loading } from "../components/ui";
import { QuestionView } from "../components/QuestionView";
import { StepViewer } from "../components/StepViewer";

function useCountdown(deadline: string, serverNow: string, onExpire: () => void): number | null {
  const [left, setLeft] = useState<number | null>(null);
  const onExpireRef = useRef(onExpire);
  useEffect(() => {
    onExpireRef.current = onExpire;
  });
  useEffect(() => {
    // Offset between the server clock and the local clock, so a wrong local time cannot extend the exam.
    const offset = new Date(serverNow).getTime() - Date.now();
    let expired = false;
    const tick = () => {
      const ms = new Date(deadline).getTime() - (Date.now() + offset);
      setLeft(Math.max(0, ms));
      if (ms <= 0 && !expired) {
        expired = true;
        onExpireRef.current();
      }
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [deadline, serverNow]);
  return left;
}

function Timer({ ms }: { ms: number | null }) {
  if (ms === null) return null;
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const text = `${h > 0 ? `${h}:` : ""}${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return (
    <div className={`timer ${ms < 5 * 60 * 1000 ? "timer-low" : ""}`} role="timer" aria-live={ms < 60000 ? "assertive" : "off"}>
      {t("exam.time_left")} <strong>{text}</strong>
    </div>
  );
}

function Results({ view }: { view: AttemptView }) {
  const tone = view.status === "passed" ? "success" : view.status === "failed" ? "danger" : "warning";
  return (
    <Card>
      <h1>
        {view.track.title} — <Badge tone={tone}>{t(`exam.status.${view.status}`)}</Badge>
      </h1>
      <table className="table">
        <thead>
          <tr>
            <th>{t("exam.section")}</th>
            <th>{t("exam.score")}</th>
            <th>{t("exam.threshold_col")}</th>
          </tr>
        </thead>
        <tbody>
          {view.sections.map((s, i) => {
            const r = view.results[i];
            return (
              <tr key={i}>
                <td>{s.title}</td>
                <td>{r ? (r.needs_review ? t("exam.awaiting_review") : `${r.score ?? 0} %`) : t("exam.not_taken")}</td>
                <td>{s.pass_percent > 0 ? `${s.pass_percent} %` : t("exam.evaluator")}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {view.review?.comment && (
        <div className="alert">
          <strong>{t("exam.reviewer_comment")}</strong>
          <p>{view.review.comment}</p>
        </div>
      )}
      <p>{t(`exam.next.${view.status}`)}</p>
      <div className="actions">
        {view.status === "passed" && (
          <Link className="button" to="/certifications">
            {t("nav.certifications")}
          </Link>
        )}
        <Link className="button button-secondary" to={`/tracks/${view.track.slug}`}>
          {t("module.back_to_track")}
        </Link>
      </div>
    </Card>
  );
}

export function ExamPage() {
  const { id } = useParams();
  const { data: view, error, loading, setData } = useAsync(() => api.get<AttemptView>(`/api/attempts/${id}`), [id]);
  const [saveError, setSaveError] = useState<unknown>(null);
  const [submitting, setSubmitting] = useState(false);
  const timers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const submit = async (force = false) => {
    if (!view || submitting) return;
    if (!force && !confirm(t("exam.confirm_submit"))) return;
    setSubmitting(true);
    try {
      Object.values(timers.current).forEach(clearTimeout);
      const next = await api.post<AttemptView>(`/api/attempts/${view.id}/submit`, { section: view.current_section });
      setData(next);
      window.scrollTo(0, 0);
    } catch (e) {
      setSaveError(e);
    } finally {
      setSubmitting(false);
    }
  };
  const submitRef = useRef(submit);
  useEffect(() => {
    submitRef.current = submit;
  });

  const inProgress = view?.status === "in_progress";
  useEffect(() => {
    if (!inProgress) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [inProgress]);

  if (loading && !view) return <Loading />;
  if (error || !view) return <ErrorBox error={error} />;
  if (view.status !== "in_progress") return <Results view={view} />;

  const section = view.visible_sections[0];
  const answered = section.items.filter((i) => i.answer !== null && i.answer !== undefined && (Array.isArray(i.answer) ? i.answer.length > 0 : i.answer.trim() !== "")).length;

  const setAnswer = (qid: string, answer: Answer) => {
    setData({
      ...view,
      visible_sections: [{ ...section, items: section.items.map((i) => (i.question_id === qid ? { ...i, answer } : i)) }],
    });
    clearTimeout(timers.current[qid]);
    // Written answers are saved after a pause in typing; choices immediately.
    timers.current[qid] = setTimeout(
      () => {
        api
          .put(`/api/attempts/${view.id}/answers`, { question_id: qid, answer })
          .then(() => setSaveError(null))
          .catch(setSaveError);
      },
      typeof answer === "string" ? 800 : 0,
    );
  };

  return (
    <div className="exam">
      <div className="exam-bar">
        <div>
          <strong>{view.track.title}</strong> · {t("exam.section_of", { n: view.current_section + 1, total: view.sections.length })} — {section.title}
        </div>
        <ExamTimer view={view} onExpire={() => submitRef.current(true)} />
        <div className="muted">{t("exam.answered", { n: answered, total: section.items.length })}</div>
      </div>
      <ErrorBox error={saveError} />
      {section.scenarios.map((sc) => (
        <Card key={sc.id} className="case-study">
          <h2>{sc.title}</h2>
          <Html html={sc.context_html} />
          {sc.steps.map((st) => (
            <StepViewer key={st.position} step={st} total={sc.steps.length} />
          ))}
        </Card>
      ))}
      <Card>
        {section.items.map((item, i) => (
          <QuestionView
            key={item.question_id}
            id={item.question_id}
            index={i}
            promptHtml={item.prompt_html}
            format={item.format}
            choices={item.choices}
            value={item.answer}
            onChange={(a) => setAnswer(item.question_id, a)}
          />
        ))}
      </Card>
      <div className="actions">
        <button className="button" onClick={() => submit()} disabled={submitting}>
          {view.current_section + 1 < view.sections.length ? t("exam.submit_section") : t("exam.submit_exam")}
        </button>
      </div>
    </div>
  );
}

function ExamTimer({ view, onExpire }: { view: AttemptView; onExpire: () => void }) {
  const ms = useCountdown(view.section_deadline, view.server_now, onExpire);
  return <Timer ms={ms} />;
}
