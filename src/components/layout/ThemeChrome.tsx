import { useEffect, useRef } from "react";

/**
 * Purely decorative chrome that only some themes render. Everything here is
 * aria-hidden and non-interactive; the real controls live in AppShell.
 */

const RAIN_GLYPHS = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789FLAC";

/** Falling-code canvas behind the Matrix theme. Pauses under reduced-motion. */
export function MatrixRain() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;

    const fontSize = 14;
    let drops: number[] = [];
    let raf = 0;
    let last = 0;

    const resize = () => {
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
      const columns = Math.ceil(canvas.width / fontSize);
      drops = Array.from({ length: columns }, () => Math.floor(Math.random() * -60));
    };

    const frame = (now: number) => {
      raf = window.requestAnimationFrame(frame);
      if (now - last < 70) return;
      last = now;
      ctx.fillStyle = "rgba(0, 0, 0, 0.08)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.font = `${fontSize}px "Consolas", "Lucida Console", monospace`;
      for (let i = 0; i < drops.length; i++) {
        const y = drops[i] * fontSize;
        const glyph = RAIN_GLYPHS[Math.floor(Math.random() * RAIN_GLYPHS.length)];
        ctx.fillStyle = "#b8ffc8";
        ctx.fillText(glyph, i * fontSize, y);
        ctx.fillStyle = "#00ff41";
        ctx.fillText(glyph, i * fontSize, y - fontSize);
        drops[i] = y > canvas.height && Math.random() > 0.975 ? 0 : drops[i] + 1;
      }
    };

    resize();
    window.addEventListener("resize", resize);
    raf = window.requestAnimationFrame(frame);
    return () => {
      window.cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
    };
  }, []);

  return <canvas ref={ref} className="matrix-rain" aria-hidden="true" />;
}

const VIS_BARS = Array.from({ length: 19 }, (_, i) => i);

/** Winamp-style spectrum analyser. Bars bounce while a run is active. */
export function WinampVis({ active }: { active: boolean }) {
  return (
    <span className={`wa-vis ${active ? "is-active" : ""}`} aria-hidden="true">
      {VIS_BARS.map((i) => (
        <span key={i} className="wa-vis-bar" style={{ animationDelay: `${(i * 137) % 900}ms` }} />
      ))}
    </span>
  );
}
