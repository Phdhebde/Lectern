import { useRef, useState, type PointerEvent } from "react";
import { t } from "../lib/i18n";
import type { Annotation } from "../lib/types";
import { AnnotatedImage } from "./AnnotatedImage";

type Mode = "box" | "arrow" | "marker";

const round = (v: number) => Math.round(Math.min(100, Math.max(0, v)) * 10) / 10;

/** Draw boxes, arrows and numbered markers on a screenshot with the mouse. */
export function AnnotationEditor({ src, value, onChange }: { src: string; value: Annotation[]; onChange: (a: Annotation[]) => void }) {
  const [mode, setMode] = useState<Mode>("box");
  const [label, setLabel] = useState("");
  const [draft, setDraft] = useState<Annotation | null>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  const point = (e: PointerEvent) => {
    const r = ref.current!.getBoundingClientRect();
    return { x: round(((e.clientX - r.left) / r.width) * 100), y: round(((e.clientY - r.top) / r.height) * 100) };
  };
  const nextLabel = () => label || String(value.filter((a) => a.label).length + 1);

  const down = (e: PointerEvent) => {
    const p = point(e);
    if (mode === "marker") {
      onChange([...value, { type: "marker", x: p.x, y: p.y, label: nextLabel() }]);
      return;
    }
    (e.target as Element).setPointerCapture(e.pointerId);
    start.current = p;
  };
  const move = (e: PointerEvent) => {
    if (!start.current) return;
    const p = point(e);
    const s = start.current;
    setDraft(
      mode === "box"
        ? { type: "box", x: Math.min(s.x, p.x), y: Math.min(s.y, p.y), w: round(Math.abs(p.x - s.x)), h: round(Math.abs(p.y - s.y)) }
        : { type: "arrow", x: s.x, y: s.y, x2: p.x, y2: p.y },
    );
  };
  const up = () => {
    if (draft && ((draft.w ?? 1) > 0.5 || draft.type === "arrow")) onChange([...value, { ...draft, label: label || undefined }]);
    setDraft(null);
    start.current = null;
  };

  return (
    <div className="annotation-editor">
      <div className="inline-form">
        {(["box", "arrow", "marker"] as Mode[]).map((m) => (
          <label key={m} className="check-inline">
            <input type="radio" name="mode" checked={mode === m} onChange={() => setMode(m)} /> {t(`editor.annotation.${m}`)}
          </label>
        ))}
        <input value={label} onChange={(e) => setLabel(e.target.value)} placeholder={t("editor.annotation.label")} className="short" />
      </div>
      <div ref={ref} className="annotation-canvas" onPointerDown={down} onPointerMove={move} onPointerUp={up}>
        <AnnotatedImage src={src} alt="" annotations={draft ? [...value, draft] : value} />
      </div>
      <ul className="plain small">
        {value.map((a, i) => (
          <li key={i}>
            {t(`editor.annotation.${a.type}`)} {a.label && `« ${a.label} »`} ({a.x}, {a.y})
            <button type="button" className="button button-small button-ghost" onClick={() => onChange(value.filter((_, j) => j !== i))}>
              {t("common.delete")}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
