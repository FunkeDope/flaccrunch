import { useState, useEffect } from "react";
import { formatElapsed } from "../../lib/format";

interface ElapsedTimerProps {
  startTime: number | null;
  running: boolean;
}

export function ElapsedTimer({ startTime, running }: ElapsedTimerProps) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (!startTime || !running) return;

    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startTime) / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [startTime, running]);

  return <span>{formatElapsed(elapsed)}</span>;
}
