interface ThreadSliderProps {
  value: number;
  max: number;
  onChange: (value: number) => void;
}

export function ThreadSlider({ value, max, onChange }: ThreadSliderProps) {
  return (
    <div className="settings-group">
      <label>
        Worker Threads
        <span className="settings-value">
          {value} / {max} cores
        </span>
      </label>
      <input
        type="range"
        min={1}
        max={max}
        value={value}
        onChange={(e) => onChange(parseInt(e.target.value))}
      />
    </div>
  );
}
