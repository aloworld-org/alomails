import { useCallback, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

import { cx } from "./cx";
import { useDismiss } from "./useDismiss";

export interface ChoiceOption {
  value: string;
  label: string;
  /** Optional visual sample, used by palette-backed choices. */
  swatch?: string | undefined;
  disabled?: boolean;
}

export interface ChoicePickerProps {
  value: string;
  options: ChoiceOption[];
  placeholder: string;
  label: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

/**
 * A token-styled single-choice popup for places where a native select would
 * expose an operating-system menu that cannot carry the alo brand palette.
 * The trigger keeps combobox semantics; the popup is a keyboard-reachable
 * listbox with visible selection and no browser-default selected colour.
 */
export function ChoicePicker({
  value,
  options,
  placeholder,
  label,
  onChange,
  disabled = false,
}: ChoicePickerProps) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const rootRef = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, rootRef, close);

  const selected = options.find((option) => option.value === value);
  const enabled = options.filter((option) => option.disabled !== true);

  function openAtSelected() {
    const index = enabled.findIndex((option) => option.value === value);
    setActiveIndex(index >= 0 ? index : 0);
    setOpen(true);
  }

  function choose(option: ChoiceOption) {
    if (option.disabled === true) return;
    onChange(option.value);
    setOpen(false);
  }

  function move(step: number) {
    if (enabled.length === 0) return;
    setActiveIndex((current) => {
      const start = current < 0 ? 0 : current;
      return (start + step + enabled.length) % enabled.length;
    });
  }

  return (
    <div className="relative w-full" ref={rootRef}>
      <button
        type="button"
        className={cx(
          "flex h-control w-full items-center justify-between gap-3 rounded-md !border !border-default bg-surface !px-4 !py-2 text-left text-base text-primary transition-[border-color,box-shadow,background-color]",
          "hover:!border-accent/40 focus-visible:!border-accent focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--accent)]",
          "disabled:cursor-not-allowed disabled:bg-raised disabled:text-tertiary",
          open && "!border-accent shadow-[inset_0_0_0_1px_var(--accent)]",
        )}
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : openAtSelected())}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            if (!open) openAtSelected();
            else move(event.key === "ArrowDown" ? 1 : -1);
          } else if (event.key === "Enter" && open && activeIndex >= 0) {
            event.preventDefault();
            const option = enabled[activeIndex];
            if (option !== undefined) choose(option);
          } else if (event.key === "Home" && open) {
            event.preventDefault();
            setActiveIndex(0);
          } else if (event.key === "End" && open) {
            event.preventDefault();
            setActiveIndex(Math.max(0, enabled.length - 1));
          }
        }}
      >
        {selected?.swatch !== undefined && (
          <span
            className="size-4 shrink-0 rounded-full border border-default shadow-sm"
            style={{ backgroundColor: selected.swatch }}
            aria-hidden="true"
          />
        )}
        <span className={cx("min-w-0 flex-1 truncate", selected === undefined && "text-tertiary")}>
          {selected?.label ?? placeholder}
        </span>
        <ChevronDown
          className={cx("size-4 shrink-0 text-tertiary transition-transform", open && "rotate-180")}
          aria-hidden="true"
        />
      </button>

      {open && (
        <div
          className="absolute left-0 right-0 top-full z-[var(--z-overlay)] mt-2 max-h-64 overflow-y-auto rounded-xl border border-default bg-surface p-2 shadow-lg"
          role="listbox"
          aria-label={label}
        >
          {options.map((option) => {
            const optionIndex = enabled.findIndex((item) => item.value === option.value);
            const isSelected = option.value === value;
            const isActive = optionIndex === activeIndex;
            return (
              <button
                key={option.value}
                type="button"
                role="option"
                aria-selected={isSelected}
                disabled={option.disabled}
                className={cx(
                  "flex min-h-11 w-full items-center gap-3 rounded-lg !px-4 !py-2.5 text-left text-base text-primary transition-colors",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/30",
                  // Two different facts, two different looks. The fill follows
                  // the pointer / arrow keys and marks exactly one row: the one
                  // Enter or a click would choose. The accent colour and the
                  // check mark belong to the current value alone. Giving both
                  // states the same fill made two rows look chosen whenever
                  // the pointer rested on a row other than the value.
                  isActive && "!bg-raised",
                  isSelected && "font-semibold !text-accent",
                  option.disabled === true && "cursor-not-allowed opacity-45",
                )}
                onMouseEnter={() => {
                  if (option.disabled !== true) setActiveIndex(optionIndex);
                }}
                onClick={() => choose(option)}
              >
                {option.swatch !== undefined && (
                  <span
                    className="size-5 shrink-0 rounded-full border border-default shadow-sm"
                    style={{ backgroundColor: option.swatch }}
                    aria-hidden="true"
                  />
                )}
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {isSelected && <Check className="size-4 shrink-0" aria-hidden="true" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
