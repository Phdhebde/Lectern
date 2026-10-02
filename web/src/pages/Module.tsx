import { useCallback, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { t } from "../lib/i18n";
import type { Answer, Question, QuizResult } from "../lib/types";
import { Badge, Card, ErrorBox, Html, Loading } from "../components/ui";
import { QuestionView } from "../components/QuestionView";
import { VideoPlayer } from "../components/VideoPlayer";

interface ModuleDetail {
  id: string;
  slug: string;
  position: number;
  title: string;
  track: { slug: string; title: string };
  video_url: string | null;
  captions_url: string | null;
  duration_minutes: number;
  body_html: string;
  attachments: { asset_id: string; label: string; name: string }[];
  doc_url: string | null;
  quiz: Question[];
  quiz_pass_percent: number;
  progress: { video_position: number; content_completed: boolean; quiz_best_score: number | null; completed: boolean } | null;
  previous: { slug: string; title: string } | null;
  next: { slug: string; title: string } | null;
}

export function ModulePage() {
  const { slug, module } = useParams();
  const { data: m, error, loading, reload } = useAsync(() => api.get<ModuleDetail>(`/api/tracks/${slug}/modules/${module}`), [slug, module]);
  const [answers, setAnswers] = useState<Record<string, Answer>>({});
  const [result, setResult] = useState<QuizResult | null>(null);
  const [quizError, setQuizError] = useState<unknown>(null);
  const markedDone = useRef(false);

  const saveProgress = useCallback(
    (body: { video_position?: number; content_completed?: boolean }) => {
      if (!m) return;
      api.post(`/api/modules/${m.id}/progress`, body).catch(() => undefined);
    },
    [m],
  );

  const markContentDone = async () => {
    if (!m || markedDone.current) return;
    markedDone.current = true;
    await api.post(`/api/modules/${m.id}/progress`, { content_completed: true });
    reload();
  };

  const submitQuiz = async () => {
    if (!m) return;
    setQuizError(null);
    try {
      const r = await api.post<QuizResult>(`/api/modules/${m.id}/quiz`, { answers });
      setResult(r);
      reload();
    } catch (e) {
      setQuizError(e);
    }
  };

  if (loading && !m) return <Loading />;
  if (error || !m) return <ErrorBox error={error} />;
  const contentDone = m.progress?.content_completed ?? false;

  return (
    <article className="module">
      <p className="breadcrumb">
        <Link to={`/tracks/${m.track.slug}`}>{m.track.title}</Link> › {t("module.n", { n: m.position })}
      </p>
      <h1>
        {m.title} {m.progress?.completed && <Badge tone="success">{t("track.completed")}</Badge>}
      </h1>

      {m.video_url && (
        <VideoPlayer
          src={m.video_url}
          captions={m.captions_url}
          startAt={m.progress?.video_position ?? 0}
          onProgress={(s) => saveProgress({ video_position: s })}
          onEnded={markContentDone}
        />
      )}

      <Card>
        <h2>{t("module.sheet")}</h2>
        <Html html={m.body_html} />
        {m.attachments.length > 0 && (
          <>
            <h3>{t("module.attachments")}</h3>
            <ul>
              {m.attachments.map((a) => (
                <li key={a.asset_id}>
                  <a href={`/api/assets/${a.asset_id}`} download={a.name}>
                    {a.label || a.name}
                  </a>
                </li>
              ))}
            </ul>
          </>
        )}
        {m.doc_url && (
          <p>
            <a href={m.doc_url} target="_blank" rel="noopener noreferrer">
              {t("module.documentation")}
            </a>
          </p>
        )}
        {!contentDone && (
          <button className="button button-secondary" onClick={markContentDone}>
            {m.video_url ? t("module.mark_watched") : t("module.mark_read")}
          </button>
        )}
      </Card>

      {m.quiz.length > 0 && (
        <Card>
          <h2>{t("module.quiz")}</h2>
          <p className="muted">
            {t("module.quiz_threshold", { n: m.quiz_pass_percent })}
            {m.progress?.quiz_best_score != null && <> · {t("module.best_score", { n: m.progress.quiz_best_score })}</>}
          </p>
          {m.quiz.map((q, i) => (
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
                setAnswers((prev) => ({ ...prev, [q.id]: a }));
              }}
              correction={result?.results.find((r) => r.question_id === q.id)}
            />
          ))}
          {result && (
            <div className={`alert ${result.passed ? "alert-success" : "alert-warning"}`} role="status">
              {t("module.quiz_score", { n: result.score })} — {result.passed ? t("module.quiz_passed") : t("module.quiz_failed")}
            </div>
          )}
          <ErrorBox error={quizError} />
          <button className="button" onClick={submitQuiz}>
            {result ? t("module.quiz_retry") : t("module.quiz_submit")}
          </button>
        </Card>
      )}

      <nav className="pager" aria-label={t("module.navigation")}>
        {m.previous ? <Link to={`/tracks/${m.track.slug}/modules/${m.previous.slug}`}>← {m.previous.title}</Link> : <span />}
        {m.next ? (
          <Link to={`/tracks/${m.track.slug}/modules/${m.next.slug}`}>{m.next.title} →</Link>
        ) : (
          <Link to={`/tracks/${m.track.slug}`}>{t("module.back_to_track")} →</Link>
        )}
      </nav>
    </article>
  );
}
