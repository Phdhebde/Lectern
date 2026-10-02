import { useEffect, useRef } from "react";
import { t } from "../lib/i18n";

interface Props {
  src: string;
  captions: string | null;
  startAt: number;
  onProgress: (seconds: number) => void;
  onEnded: () => void;
}

/**
 * Standard HTML5 player. HLS streams (.m3u8) use the browser's native support or
 * hls.js; the video is served by the instance's own storage, never a public platform.
 */
export function VideoPlayer({ src, captions, startAt, onProgress, onEnded }: Props) {
  const ref = useRef<HTMLVideoElement>(null);
  const lastSaved = useRef(0);

  useEffect(() => {
    const video = ref.current;
    if (!video) return;
    let destroy: (() => void) | undefined;
    if (src.includes(".m3u8") && !video.canPlayType("application/vnd.apple.mpegurl")) {
      import("hls.js").then(({ default: Hls }) => {
        if (!Hls.isSupported()) return;
        const hls = new Hls();
        hls.loadSource(src);
        hls.attachMedia(video);
        destroy = () => hls.destroy();
      });
    } else {
      video.src = src;
    }
    const seek = () => {
      if (startAt > 0 && startAt < video.duration - 5) video.currentTime = startAt;
    };
    video.addEventListener("loadedmetadata", seek, { once: true });
    return () => {
      video.removeEventListener("loadedmetadata", seek);
      destroy?.();
    };
  }, [src, startAt]);

  const timeUpdate = () => {
    const v = ref.current;
    if (!v) return;
    // Save the position every 15 seconds so learners resume where they stopped.
    if (Math.abs(v.currentTime - lastSaved.current) > 15) {
      lastSaved.current = v.currentTime;
      onProgress(Math.floor(v.currentTime));
    }
    if (v.duration && v.currentTime / v.duration > 0.95) onEnded();
  };

  return (
    <video ref={ref} className="video" controls preload="metadata" onTimeUpdate={timeUpdate} onEnded={onEnded} crossOrigin="anonymous">
      {captions && <track kind="captions" src={captions} srcLang={document.documentElement.lang} label={t("module.captions")} default />}
      {t("module.no_video_support")}
    </video>
  );
}
