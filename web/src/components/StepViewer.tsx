import { t } from "../lib/i18n";
import type { Step } from "../lib/types";
import { AnnotatedImage } from "./AnnotatedImage";
import { Html } from "./ui";

export function StepViewer({ step, total }: { step: Step; total: number }) {
  return (
    <div className="step">
      <p className="step-counter">{t("scenario.step_of", { n: step.position, total })}</p>
      <div className="step-action">
        <h3>{t("scenario.action")}</h3>
        <Html html={step.action_html} />
      </div>
      {step.image_url && <AnnotatedImage src={step.image_url} alt={step.image_alt} annotations={step.annotations} />}
      {step.expected_html && step.expected_html.trim() && (
        <div className="step-expected">
          <h3>{t("scenario.expected")}</h3>
          <Html html={step.expected_html} />
        </div>
      )}
    </div>
  );
}
