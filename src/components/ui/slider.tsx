import * as React from "react";
import { cn } from "@/lib/utils";

interface SliderProps {
  value: number[];
  onValueChange: (value: number[]) => void;
  /** Fires once when the user releases the pointer / blurs the slider — use this to persist. */
  onValueCommit?: (value: number[]) => void;
  min?: number;
  max?: number;
  step?: number;
  className?: string;
}

const Slider = React.forwardRef<HTMLDivElement, SliderProps>(
  ({ value, onValueChange, onValueCommit, min = 0, max = 100, step = 1, className }, ref) => {
    const percentage = ((value[0] - min) / (max - min)) * 100;

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      onValueChange([Number(e.target.value)]);
    };

    const commit = (e: React.SyntheticEvent<HTMLInputElement>) => {
      if (onValueCommit) {
        onValueCommit([Number((e.target as HTMLInputElement).value)]);
      }
    };

    return (
      <div ref={ref} className={cn("relative flex w-full touch-none select-none items-center", className)}>
        <div className="relative h-1.5 w-full grow overflow-hidden rounded-full bg-primary/20">
          <div
            className="absolute h-full bg-primary"
            style={{ width: `${percentage}%` }}
          />
        </div>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value[0]}
          onChange={handleChange}
          onPointerUp={commit}
          onKeyUp={commit}
          onBlur={commit}
          className="absolute w-full opacity-0 cursor-pointer"
          style={{ height: "1.5rem" }}
        />
        <div
          className="absolute block h-4 w-4 rounded-full border border-primary/50 bg-background shadow transition-colors pointer-events-none"
          style={{ left: `calc(${percentage}% - 8px)` }}
        />
      </div>
    );
  }
);
Slider.displayName = "Slider";

export { Slider };
