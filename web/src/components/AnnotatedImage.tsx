import type { Annotation } from "../lib/types";

/**
 * Screenshot with annotations drawn on top. Coordinates are percentages, so the
 * overlay scales with the image. Colours come from the --color-annotation token.
 */
export function AnnotatedImage({ src, alt, annotations }: { src: string; alt: string; annotations: Annotation[] }) {
  return (
    <figure className="annotated">
      <img src={src} alt={alt} loading="lazy" />
      <svg className="annotated-overlay" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <marker id="arrowhead" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" className="annotation-fill" />
          </marker>
        </defs>
        {annotations.map((a, i) =>
          a.type === "box" ? (
            <rect key={i} x={a.x} y={a.y} width={a.w ?? 0} height={a.h ?? 0} className="annotation-stroke" vectorEffect="non-scaling-stroke" />
          ) : a.type === "arrow" ? (
            <line key={i} x1={a.x} y1={a.y} x2={a.x2 ?? a.x} y2={a.y2 ?? a.y} className="annotation-stroke" markerEnd="url(#arrowhead)" vectorEffect="non-scaling-stroke" />
          ) : null,
        )}
      </svg>
      {annotations.map((a, i) => {
        const label = a.label ?? (a.type === "marker" ? String(i + 1) : undefined);
        if (!label) return null;
        // Labels sit at the start of boxes/arrows and on markers.
        return (
          <span key={`l${i}`} className={`annotation-label ${a.type === "marker" ? "annotation-marker" : ""}`} style={{ left: `${a.x}%`, top: `${a.y}%` }}>
            {label}
          </span>
        );
      })}
      {alt && <figcaption className="sr-only">{alt}</figcaption>}
    </figure>
  );
}
