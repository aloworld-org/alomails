import { Check, Copy, Pipette, Plus } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";

import { cx } from "./cx";
import { IconButton } from "./IconButton";
import { useDismiss } from "./useDismiss";

interface Rgb { r: number; g: number; b: number }
interface Hsv { h: number; s: number; v: number }

const PRESETS = ["#102A43", "#E76F51", "#F3F0EA", "#CBD5E1"];

function normalizeHex(value: string): string {
  const prefixed = value.startsWith("#") ? value : `#${value}`;
  return /^#[0-9a-f]{6}$/i.test(prefixed) ? prefixed.toUpperCase() : "#102A43";
}

function hexToRgb(hex: string): Rgb {
  const safe = normalizeHex(hex).slice(1);
  return {
    r: Number.parseInt(safe.slice(0, 2), 16),
    g: Number.parseInt(safe.slice(2, 4), 16),
    b: Number.parseInt(safe.slice(4, 6), 16),
  };
}

function rgbToHex({ r, g, b }: Rgb): string {
  return `#${[r, g, b].map((part) => Math.max(0, Math.min(255, Math.round(part))).toString(16).padStart(2, "0")).join("")}`.toUpperCase();
}

function rgbToHsv({ r, g, b }: Rgb): Hsv {
  const red = r / 255;
  const green = g / 255;
  const blue = b / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const delta = max - min;
  let h = 0;
  if (delta !== 0) {
    if (max === red) h = 60 * (((green - blue) / delta) % 6);
    else if (max === green) h = 60 * ((blue - red) / delta + 2);
    else h = 60 * ((red - green) / delta + 4);
  }
  return { h: h < 0 ? h + 360 : h, s: max === 0 ? 0 : delta / max, v: max };
}

function hsvToRgb({ h, s, v }: Hsv): Rgb {
  const chroma = v * s;
  const x = chroma * (1 - Math.abs(((h / 60) % 2) - 1));
  const match = v - chroma;
  let parts: [number, number, number];
  if (h < 60) parts = [chroma, x, 0];
  else if (h < 120) parts = [x, chroma, 0];
  else if (h < 180) parts = [0, chroma, x];
  else if (h < 240) parts = [0, x, chroma];
  else if (h < 300) parts = [x, 0, chroma];
  else parts = [chroma, 0, x];
  return { r: (parts[0] + match) * 255, g: (parts[1] + match) * 255, b: (parts[2] + match) * 255 };
}

export interface ColorPickerProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  triggerIcon?: ReactNode;
  disabled?: boolean;
  className?: string;
  triggerClassName?: string;
  formatKey?: string;
  onPointerDown?: () => void;
  resetLabel?: string;
  onReset?: () => void;
}

export function ColorPicker({
  label,
  value,
  onChange,
  triggerIcon,
  disabled = false,
  className,
  triggerClassName,
  formatKey,
  onPointerDown,
  resetLabel,
  onReset,
}: ColorPickerProps) {
  const [open, setOpen] = useState(false);
  const [savedColors, setSavedColors] = useState<string[]>([]);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, root, close);
  const hex = normalizeHex(value);
  const rgb = useMemo(() => hexToRgb(hex), [hex]);
  const hsv = useMemo(() => rgbToHsv(rgb), [rgb]);
  const presets = useMemo(() => [...PRESETS, ...savedColors.filter((color) => !PRESETS.includes(color))], [savedColors]);

  useEffect(() => {
    try {
      const stored = JSON.parse(window.localStorage.getItem("alo-saved-colours") ?? "[]") as unknown;
      if (Array.isArray(stored)) {
        setSavedColors(stored.filter((item): item is string => typeof item === "string" && /^#[0-9a-f]{6}$/i.test(item)).map(normalizeHex).slice(0, 8));
      }
    } catch {
      setSavedColors([]);
    }
  }, []);

  const updateHsv = (next: Hsv) => onChange(rgbToHex(hsvToRgb(next)));
  const updateRgb = (channel: keyof Rgb, next: string) => {
    const parsed = Number.parseInt(next, 10);
    if (Number.isNaN(parsed)) return;
    onChange(rgbToHex({ ...rgb, [channel]: Math.max(0, Math.min(255, parsed)) }));
  };
  const updateCanvas = (event: ReactPointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    updateHsv({
      h: hsv.h,
      s: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
      v: Math.max(0, Math.min(1, 1 - (event.clientY - bounds.top) / bounds.height)),
    });
  };
  const useEyedropper = async () => {
    const EyeDropperCtor = (window as Window & { EyeDropper?: new () => { open: () => Promise<{ sRGBHex: string }> } }).EyeDropper;
    if (EyeDropperCtor === undefined) return;
    try {
      const result = await new EyeDropperCtor().open();
      onChange(normalizeHex(result.sRGBHex));
    } catch {
      // Cancelling the eyedropper leaves the current colour unchanged.
    }
  };

  return (
    <div ref={root} className={cx("relative inline-flex", className)} data-format-key={formatKey}>
      <button
        type="button"
        disabled={disabled}
        aria-label={label}
        aria-haspopup="dialog"
        aria-expanded={open}
        className={cx(
          "inline-flex size-10 items-center justify-center rounded-lg border border-default bg-surface text-primary transition-colors hover:border-accent hover:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20 disabled:pointer-events-none disabled:opacity-50",
          triggerIcon === undefined && "p-1.5",
          triggerClassName,
        )}
        onPointerDown={onPointerDown}
        onClick={() => setOpen((current) => !current)}
      >
        {triggerIcon === undefined ? (
          <span className="size-full rounded-md border border-black/10" style={{ backgroundColor: hex }} />
        ) : (
          <span className="relative grid size-full place-items-center pb-1">
            {triggerIcon}
            <span className="absolute inset-x-1 bottom-0 h-0.5 rounded-full" style={{ backgroundColor: hex }} />
          </span>
        )}
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={label}
          className="absolute left-0 top-full z-50 mt-2 w-[min(22rem,calc(100vw-2rem))] overflow-hidden rounded-2xl border border-default bg-surface shadow-lg"
        >
          <div
            className="relative h-44 cursor-crosshair touch-none bg-[linear-gradient(to_bottom,transparent,#000),linear-gradient(to_right,#fff,transparent)]"
            style={{ backgroundColor: `hsl(${hsv.h} 100% 50%)` }}
            onPointerDown={(event) => {
              event.currentTarget.setPointerCapture(event.pointerId);
              updateCanvas(event);
            }}
            onPointerMove={(event) => {
              if (event.currentTarget.hasPointerCapture(event.pointerId)) updateCanvas(event);
            }}
          >
            <span
              className="pointer-events-none absolute size-6 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-md"
              style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, backgroundColor: hex }}
            />
          </div>
          <div className="space-y-5 p-5">
            <div className="flex items-center gap-4">
              <IconButton label="Pick a colour from the screen" icon={<Pipette />} onClick={() => void useEyedropper()} />
              <span className="size-11 shrink-0 rounded-full border border-default" style={{ backgroundColor: hex }} />
              <input
                type="range"
                min="0"
                max="360"
                value={Math.round(hsv.h)}
                aria-label="Hue"
                className="h-3 min-w-0 flex-1 cursor-pointer appearance-none rounded-full bg-[linear-gradient(to_right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)] accent-accent [&::-moz-range-thumb]:size-6 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border-2 [&::-moz-range-thumb]:border-white [&::-moz-range-thumb]:bg-accent [&::-webkit-slider-thumb]:size-6 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-white [&::-webkit-slider-thumb]:bg-accent [&::-webkit-slider-thumb]:shadow-md"
                onChange={(event) => updateHsv({ ...hsv, h: Number(event.target.value) })}
              />
            </div>
            <div className="grid grid-cols-3 gap-3">
              {(["r", "g", "b"] as const).map((channel) => (
                <label key={channel} className="grid gap-1.5 text-center text-xs font-semibold uppercase text-secondary">
                  <input
                    inputMode="numeric"
                    value={Math.round(rgb[channel])}
                    aria-label={`${channel.toUpperCase()} value`}
                    className="h-11 min-w-0 rounded-xl border border-default bg-surface px-3 text-center text-base font-medium text-primary outline-none focus:border-accent focus:ring-2 focus:ring-accent/10"
                    onChange={(event) => updateRgb(channel, event.target.value)}
                  />
                  {channel}
                </label>
              ))}
            </div>
          </div>
          <div className="flex items-center gap-3 border-t border-subtle px-5 py-4">
            <span className="text-xs font-semibold text-secondary">HEX</span>
            <input
              value={hex}
              aria-label="Hex colour"
              className="h-10 min-w-0 flex-1 rounded-xl border border-default bg-surface px-3 font-mono text-sm font-medium uppercase text-primary outline-none focus:border-accent focus:ring-2 focus:ring-accent/10"
              onChange={(event) => {
                const next = event.target.value.startsWith("#") ? event.target.value : `#${event.target.value}`;
                if (/^#[0-9a-f]{6}$/i.test(next)) onChange(next.toUpperCase());
              }}
            />
            <IconButton label="Copy hex colour" icon={<Copy />} onClick={() => void navigator.clipboard?.writeText(hex)} />
            <div className="ml-auto flex items-center gap-2">
              {presets.slice(0, 4).map((preset) => (
                <button
                  key={preset}
                  type="button"
                  aria-label={`Use ${preset}`}
                  className="grid size-9 place-items-center rounded-full border border-default"
                  style={{ backgroundColor: preset }}
                  onClick={() => onChange(preset)}
                >
                  {hex === preset && <Check className="size-4 text-white drop-shadow" />}
                </button>
              ))}
              <button
                type="button"
                aria-label="Save current colour"
                className="grid size-9 place-items-center rounded-lg border border-default text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
                onClick={() => {
                  const next = [hex, ...savedColors.filter((item) => item !== hex)].slice(0, 8);
                  setSavedColors(next);
                  window.localStorage.setItem("alo-saved-colours", JSON.stringify(next));
                }}
              >
                <Plus className="size-4" />
              </button>
            </div>
          </div>
          {onReset !== undefined && (
            <div className="border-t border-subtle px-5 py-3">
              <button
                type="button"
                className="min-h-10 rounded-lg px-3 text-sm font-medium text-secondary transition-colors hover:bg-accent-soft hover:text-accent"
                onClick={() => {
                  onReset();
                  setOpen(false);
                }}
              >
                {resetLabel ?? "Use default colour"}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
