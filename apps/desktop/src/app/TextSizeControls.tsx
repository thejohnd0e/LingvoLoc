import {
  clampTextScale,
  saveTextScale,
  textScaleMax,
  textScaleMin,
  textScaleStep,
} from '../lib/settings';

interface TextSizeControlsProps {
  value: number;
  onChange: (value: number) => void;
  onSave?: (value: number) => void;
}

export default function TextSizeControls({
  value,
  onChange,
  onSave = saveTextScale,
}: TextSizeControlsProps) {
  function changeBy(delta: number) {
    const next = clampTextScale(value + delta);
    onSave(next);
    onChange(next);
  }

  return (
    <div className="text-size-controls" aria-label="Text size">
      <button
        className="text-size-button"
        type="button"
        aria-label="Decrease text size"
        title="Decrease text size"
        disabled={value <= textScaleMin}
        onClick={() => changeBy(-textScaleStep)}
      >
        -
      </button>
      <span aria-live="polite">{Math.round(value * 100)}%</span>
      <button
        className="text-size-button"
        type="button"
        aria-label="Increase text size"
        title="Increase text size"
        disabled={value >= textScaleMax}
        onClick={() => changeBy(textScaleStep)}
      >
        +
      </button>
    </div>
  );
}
