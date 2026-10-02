import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { formatDateTime, t } from "../lib/i18n";
import type { AttemptView } from "../lib/types";
import { Card, ErrorBox, Html, Loading } from "../components/ui";
import { StepViewer } from "../components/StepViewer";

interface Pending {
  id: string;
  finished_at: string;
  track_title: string;
  learner: string;
  organization: string | null;
}

export function ReviewsPage() {
  const { data, error, loading } = useAsync(() => api.get<Pending[]>("/api/reviews"), []);
  return (
    <>
      <h1>{t("reviews.title")}</h1>
      {loading && <Loading />}
      <ErrorBox error={error} />
      {data?.length === 0 && <p className="muted">{t("reviews.none")}</p>}
      <ul className="plain">
        {data?.map((p) => (
          <li key={p.id}>
            <Link to={`/reviews/${p.id}`}>
              {p.track_title} — {p.learner}
            </Link>{" "}
            <span className="muted small">
              {p.organization} · {formatDateTime(p.finished_at)}
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}

const DEFAULT_GRID = ["reviews.criteria.diagnosis", "reviews.criteria.solution", "reviews.criteria.architecture", "reviews.criteria.interview"];

export function ReviewDetail() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { data: v, error, loading } = useAsync(() => api.get<AttemptView>(`/api/reviews/${id}`), [id]);
  const [grid, setGrid] = useState(DEFAULT_GRID.map((k) => ({ criterion: t(k), score: 0, max: 5 })));
  const [comment, setComment] = useState("");
  const [submitError, setSubmitError] = useState<unknown>(null);
  if (loading && !v) return <Loading />;
  if (error || !v) return <ErrorBox error={error} />;

  const decide = async (decision: "pass" | "fail") => {
    if (!confirm(t(`reviews.confirm_${decision}`))) return;
    try {
      await api.post(`/api/reviews/${v.id}`, { decision, comment, grid });
      navigate("/reviews");
    } catch (e) {
      setSubmitError(e);
    }
  };

  return (
    <>
      <p className="breadcrumb">
        <Link to="/reviews">{t("reviews.title")}</Link>
      </p>
      <h1>
        {v.track.title} — {v.learner?.name}
      </h1>
      {v.visible_sections.map((s) => (
        <section key={s.index}>
          <h2>{s.title}</h2>
          {s.scenarios.map((sc) => (
            <Card key={sc.id}>
              <h3>{sc.title}</h3>
              <Html html={sc.context_html} />
              {sc.steps.map((st) => (
                <StepViewer key={st.position} step={st} total={sc.steps.length} />
              ))}
            </Card>
          ))}
          {s.items.map((item, i) => (
            <Card key={item.question_id}>
              <p className="question-number">{t("quiz.question_n", { n: i + 1 })}</p>
              <Html html={item.prompt_html} />
              {item.format === "written" ? (
                <blockquote className="written-answer">{typeof item.answer === "string" ? item.answer : t("reviews.no_answer")}</blockquote>
              ) : (
                <ul>
                  {item.choices.map((c) => (
                    <li key={c.id} className={c.correct ? "choice-right" : ""}>
                      {Array.isArray(item.answer) && item.answer.includes(c.id) ? "☑ " : "☐ "}
                      <Html html={c.html} className="choice-text inline" />
                    </li>
                  ))}
                </ul>
              )}
              {item.explanation_html && <Html html={item.explanation_html} className="prose muted" />}
            </Card>
          ))}
        </section>
      ))}
      <Card>
        <h2>{t("reviews.evaluation")}</h2>
        <table className="table compact">
          <tbody>
            {grid.map((g, i) => (
              <tr key={i}>
                <td>
                  <input aria-label={t("reviews.criterion")} value={g.criterion} onChange={(e) => setGrid(grid.map((x, j) => (j === i ? { ...x, criterion: e.target.value } : x)))} />
                </td>
                <td>
                  <input
                    type="number"
                    aria-label={t("reviews.score")}
                    min={0}
                    max={g.max}
                    value={g.score}
                    onChange={(e) => setGrid(grid.map((x, j) => (j === i ? { ...x, score: Number(e.target.value) } : x)))}
                  />{" "}
                  / {g.max}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <label className="form">
          {t("reviews.comment")}
          <textarea rows={5} value={comment} onChange={(e) => setComment(e.target.value)} />
        </label>
        <ErrorBox error={submitError} />
        <div className="actions">
          <button className="button" onClick={() => decide("pass")}>
            {t("reviews.pass")}
          </button>
          <button className="button button-danger" onClick={() => decide("fail")}>
            {t("reviews.fail")}
          </button>
        </div>
      </Card>
    </>
  );
}
