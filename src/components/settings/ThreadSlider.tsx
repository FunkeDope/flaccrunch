interface ThreadSliderProps {
  value: number;
  max: number;
  onChange: (value: number) => void;
}

export function ThreadSlider({ value, max, onChange }: ThreadSliderProps) {
  return (
    <div className="settings-group">
      <label className="settings-slider-head" htmlFor="worker-threads">
        <span>Worker Threads</span>
        <span className="settings-value">{value} / {max} cores</span>
      </label>
      <input
        id="worker-threads"
        className="settings-slider"
        type="range"
        min={1}
        max={max}
        value={value}
        onChange={(e) => onChange(parseInt(e.target.value))}
      />
      <div className="settings-hint">
        Files encoded in parallel. Leaving one core free keeps the app responsive.
      </div>
    </div>
  );
}
