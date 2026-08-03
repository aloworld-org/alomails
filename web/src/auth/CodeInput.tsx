// A segmented numeric code input (the 2FA screen's six boxes). Auto-advances on
// entry, steps back on Backspace, supports paste of a full code, and calls
// onComplete when all digits are present. Controlled via a single string value.
import { useRef } from "react";
import type { ChangeEvent, ClipboardEvent, KeyboardEvent } from "react";

import styles from "./CodeInput.module.css";

interface CodeInputProps {
  length?: number;
  value: string;
  onChange: (value: string) => void;
  onComplete?: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
}

export function CodeInput({
  length = 6,
  value,
  onChange,
  onComplete,
  disabled = false,
  ariaLabel,
}: CodeInputProps) {
  const refs = useRef<Array<HTMLInputElement | null>>([]);

  function setChar(index: number, char: string): string {
    const chars = value.split("");
    while (chars.length < length) chars.push("");
    chars[index] = char;
    const joined = chars.join("").slice(0, length);
    onChange(joined);
    return joined;
  }

  function handleChange(index: number, event: ChangeEvent<HTMLInputElement>) {
    const digit = event.target.value.replace(/\D/g, "").slice(-1);
    if (digit === "") return;
    const joined = setChar(index, digit);
    if (index < length - 1) refs.current[index + 1]?.focus();
    if (joined.length === length && !joined.includes("") && onComplete) onComplete(joined);
  }

  function handleKeyDown(index: number, event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Backspace") {
      if ((value[index] ?? "") === "" && index > 0) {
        event.preventDefault();
        setChar(index - 1, "");
        refs.current[index - 1]?.focus();
      } else {
        setChar(index, "");
      }
    } else if (event.key === "ArrowLeft" && index > 0) {
      refs.current[index - 1]?.focus();
    } else if (event.key === "ArrowRight" && index < length - 1) {
      refs.current[index + 1]?.focus();
    }
  }

  function handlePaste(event: ClipboardEvent<HTMLInputElement>) {
    const digits = event.clipboardData.getData("text").replace(/\D/g, "").slice(0, length);
    if (digits === "") return;
    event.preventDefault();
    onChange(digits);
    refs.current[Math.min(digits.length, length - 1)]?.focus();
    if (digits.length === length && onComplete) onComplete(digits);
  }

  return (
    <div className={styles.group} role="group" aria-label={ariaLabel}>
      {Array.from({ length }, (_, i) => (
        <input
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          className={styles.box}
          type="text"
          inputMode="numeric"
          maxLength={1}
          value={value[i] ?? ""}
          onChange={(e) => handleChange(i, e)}
          onKeyDown={(e) => handleKeyDown(i, e)}
          onPaste={handlePaste}
          disabled={disabled}
          autoComplete={i === 0 ? "one-time-code" : "off"}
          aria-label={`${ariaLabel} ${i + 1}`}
        />
      ))}
    </div>
  );
}
