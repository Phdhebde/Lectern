import type { Answer, Choice } from "../lib/types";
import { t } from "../lib/i18n";
import { Html } from "./ui";

interface Props {
  id: string;
  index: number;
  promptHtml: string;
  format: "choice" | "written";
  choices: Choice[];
  value: Answer | null | undefined;
  onChange: (a: Answer) => void;
  disabled?: boolean;
  /** After correction: which choices were right, and whether the answer was. */
  correction?: { correct: boolean; correct_choices: string[]; explanation_html: string };
}

export function QuestionView({ id, index, promptHtml, format, choices, value, onChange, disabled, correction }: Props) {
  const selected = Array.isArray(value) ? value : [];
  const toggle = (cid: string) => {
    onChange(selected.includes(cid) ? selected.filter((x) => x !== cid) : [...selected, cid]);
  };
  return (
    <fieldset className={`question ${correction ? (correction.correct ? "is-correct" : "is-wrong") : ""}`}>
      <legend>
        <span className="question-number">{t("quiz.question_n", { n: index + 1 })}</span>
      </legend>
      <Html html={promptHtml} />
      {format === "written" ? (
        <textarea
          aria-label={t("quiz.your_answer")}
          rows={10}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          disabled={disabled}
        />
      ) : (
        <>
          <p className="hint">{t("quiz.select_all")}</p>
          <ul className="choices">
            {choices.map((c) => {
              const right = correction?.correct_choices.includes(c.id);
              return (
                <li key={c.id} className={correction ? (right ? "choice-right" : selected.includes(c.id) ? "choice-wrong" : "") : ""}>
                  <label>
                    <input
                      type="checkbox"
                      name={`q-${id}`}
                      checked={selected.includes(c.id)}
                      onChange={() => toggle(c.id)}
                      disabled={disabled}
                    />
                    <Html html={c.html} className="choice-text" />
                  </label>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {correction && (
        <div className={`alert ${correction.correct ? "alert-success" : "alert-danger"}`}>
          <strong>{correction.correct ? t("quiz.correct") : t("quiz.incorrect")}</strong>
          {correction.explanation_html && <Html html={correction.explanation_html} />}
        </div>
      )}
    </fieldset>
  );
}
