interface ProgressBarProps {
  percent: number;
}

export function ProgressBar({ percent }: ProgressBarProps) {
  return (
    <div className="progress-bar">
      <div className="fill" style={{ width: `${percent}%` }} />
    </div>
  );
}
