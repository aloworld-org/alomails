import { AlertCircle, X } from "lucide-react";
import { createPortal } from "react-dom";

import { strings } from "../i18n";
import styles from "./billingStyles";

interface ErrorBannerProps {
  message: string;
  presentation?: "inline" | "popup";
  onDismiss?: () => void;
}

/** A persistent error notice. List-level failures can rise above scroll
 * containers as a compact popup; contextual form failures stay in flow. */
export function ErrorBanner({
  message,
  presentation = "inline",
  onDismiss,
}: ErrorBannerProps) {
  if (presentation === "inline") {
    return <p className={styles.error} role="alert">{message}</p>;
  }

  return createPortal(
    <div className="pointer-events-none fixed inset-x-0 top-5 z-[70] flex justify-center px-4">
      <div
        className="pointer-events-auto flex w-full max-w-xl items-start gap-3 rounded-2xl border border-danger/20 bg-surface p-3 shadow-lg"
        role="alert"
      >
        <span
          className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-danger-tint text-danger"
          aria-hidden="true"
        >
          <AlertCircle className="size-5" />
        </span>
        <p className="m-0 min-w-0 flex-1 py-2 text-sm leading-6 text-primary">
          {message}
        </p>
        {onDismiss !== undefined && (
          <button
            type="button"
            className="flex size-9 shrink-0 items-center justify-center rounded-xl text-tertiary transition-colors hover:bg-danger-tint hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30"
            aria-label={strings.close}
            onClick={onDismiss}
          >
            <X className="size-4" aria-hidden="true" />
          </button>
        )}
      </div>
    </div>,
    document.body,
  );
}
