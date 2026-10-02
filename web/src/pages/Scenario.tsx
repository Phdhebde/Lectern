import { useEffect, useState } from "react";
import { Link, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { t } from "../lib/i18n";
import type { Answer, Question, QuizResult, Step } from "../lib/types";
import { Badge, Card, ErrorBox, Html, Loading } from "../components/ui";
import { QuestionView } from "../components/QuestionView";
import { StepViewer } from "../components/StepViewer";

interface ScenarioDetail {
  id: string;
  slug: string;
  title: string;
  kind: "implementation" | "diagnostic";
  family: string | null;
  track: { slug: string; title: string };
  context_html: string;
  pitfalls_html: string;
  steps: Step[];
  questions: Question[];
  progress: { current_step: number; completed: boolean } | null;
}

export function ScenarioPage() {
  const { slug, scenario } = useParams();
  const { data: s, error, loading, reload } = useAsync(() => api.get<ScenarioDetail>(`/api/tracks/${slug}/scenarios/${scenario}`), [slug, scenario]);
  // -1 = context screen, 0..n-1 = steps, n = verification
  const [pos, setPos] = useState(-1);
  const [answers, setAnswers] = useState<Record<string, Answer>>({});
  const [result, setResult] = useState<QuizResult | null>(null);
  const [checkError, setCheckError] = useState<unknown>(null);

  useEffect(() => {
    if (s && pos >= 0 && pos < s.steps.length) {
      api.post(`/api/scenarios/${s.id}/progress`, { step: pos + 1 }).catch(() => undefined);
    }
  }, [s, pos]);

  if (loading && !s) return <Loading />;
  if (error || !s) return <ErrorBox error={error} />;
  const total = s.steps.length;

  const check = async () => {
    setCheckError(null);
    try {
      setResult(await api.post<QuizResult>(`/api/scenarios/${s.id}/check`, { answers }));
      reload();
    } catch (e) {
      setCheckError(e);
    }
  };

  return (
    <article className="scenario">
      <p className="breadcrumb">
        <Link to={`/tracks/${s.track.slug}`}>{s.track.title}</Link> › {t("track.scenarios")}
      </p>
      <h1>
        {s.title} <Badge tone="primary">{t(`scenario.kind.${s.kind}`)}</Badge>{" "}
        {s.progress?.completed && <Badge tone="success">{t("track.completed")}</Badge>}
      </h1>
      <ol className="stepper" aria-label={t("scenario.progress")}>
        <li className={pos === -1 ? "current" : "done"}>
          <button onClick={() => setPos(-1)}>{t("scenario.context")}</button>
        </li>
        {s.steps.map((st, i) => (
          <li key={st.position} className={pos === i ? "current" : pos > i ? "done" : ""}>
            <button onClick={() => setPos(i)} aria-label={t("scenario.step_of", { n: i + 1, total })}>
              {i + 1}
            </button>
          </li>
        ))}
        <li className={pos === total ? "current" : ""}>
          <button onClick={() => setPos(total)}>{t("scenario.check")}</button>
        </li>
      </ol>

      <Card>
        {pos === -1 && (
          <>
            <h2>{t("scenario.context")}</h2>
            <Html html={s.context_html} />
            {s.pitfalls_html.trim() && (
              <div className="alert alert-warning">
                <strong>{t("scenario.pitfalls")}</strong>
                <Html html={s.pitfalls_html} />
              </div>
            )}
            <p className="muted small">{t("scenario.practice_notice")}</p>
          </>
        )}
        {pos >= 0 && pos < total && <StepViewer step={s.steps[pos]} total={total} />}
        {pos === total && (
          <>
            <h2>{t("scenario.check")}</h2>
            {s.questions.length === 0 && <p>{t("scenario.no_questions")}</p>}
            {s.questions.map((q, i) => (
              <QuestionView
                key={q.id}
                id={q.id}
                index={i}
                promptHtml={q.prompt_html}
                format={q.format}
                choices={q.choices}
                value={answers[q.id]}
                onChange={(a) => {
                  setResult(null);
                  setAnswers((p) => ({ ...p, [q.id]: a }));
                }}
                correction={result?.results.find((r) => r.question_id === q.id)}
              />
            ))}
            {result && (
              <div className={`alert ${result.passed ? "alert-success" : "alert-warning"}`} role="status">
                {result.passed ? t("scenario.completed") : t("scenario.retry")}
              </div>
            )}
            <ErrorBox error={checkError} />
            <button className="button" onClick={check}>
              {s.questions.length ? t("module.quiz_submit") : t("scenario.finish")}
            </button>
          </>
        )}
      </Card>
      <nav className="pager">
        <button className="button button-secondary" disabled={pos === -1} onClick={() => setPos(pos - 1)}>
          ← {t("scenario.previous")}
        </button>
        {pos < total ? (
          <button className="button" onClick={() => setPos(pos + 1)}>
            {t("scenario.next")} →
          </button>
        ) : (
          <Link className="button button-secondary" to={`/tracks/${s.track.slug}`}>
            {t("module.back_to_track")}
          </Link>
        )}
      </nav>
    </article>
  );
}
