// Branded, centred dialogs — our replacement for native window.confirm /
// The context itself lives in DialogContext.ts so Fast Refresh keeps its identity.
// window.prompt, which render the webview's own "tauri.localhost says" chrome in
// the desktop app (and a bare browser dialog on the web). A single provider
// renders one modal; `useDialogs()` exposes promise-returning confirm/prompt/
// alert, so call sites read like the natives they replace:
//   if (!(await confirm({ message }))) return;
//   const name = (await prompt({ message }))?.trim();
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import { strings } from "../i18n";
import { Button } from "./Button";
import {
  DialogContext,
  type Dialogs,
  type PromptOptions,
} from "./DialogContext";

type Kind = "confirm" | "prompt" | "alert";

// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The entrance keyframes are global names and live in `ds/global.css`; their
// duration and easing are `--animation-dialog-*`.

/** Warm workshop identity via tokens; never a raw browser popup. */
const SCRIM =
  "fixed inset-0 z-[var(--z-modal)] flex items-center justify-center p-6 " +
  "bg-overlay animate-[alo-dialog-scrim_var(--animation-dialog-scrim)]";

const PANEL =
  "w-full max-w-[var(--dialog-width)] flex flex-col gap-3 p-6 " +
  "bg-surface rounded-xl shadow-lg " +
  "animate-[alo-dialog-panel_var(--animation-dialog-panel)]";

const TITLE = "m-0 text-lg font-semibold text-primary";
/** `whitespace-pre-wrap`, because callers pass messages with their own line
 *  breaks and a confirm that reflows them reads as one run-on sentence. */
const MESSAGE =
  "m-0 text-secondary text-md leading-relaxed whitespace-pre-wrap";

/** The prompt's field.
 *
 *  NOTE (flagged for the D1.55 wave check): this is a second `.input` inside
 *  `ds/` — it sits on `--bg-app` and shows focus as a border plus a ring,
 *  where `ds/Input` sits on a panel and shows focus as an outline. Adopting
 *  `Input` here would change how every prompt looks, which is a restyle and
 *  not what D1.52 was contracted to do (props, behaviour and appearance
 *  unchanged), so the rules are carried across verbatim and the duplication is
 *  recorded rather than quietly kept. */
const FIELD =
  "w-full mt-1 px-3 py-2 rounded-md border border-default bg-app text-primary " +
  "font-[inherit] focus:outline-none focus:border-accent " +
  "focus:shadow-[var(--focus-ring-soft)]";

const ACTIONS = "flex justify-end gap-2 mt-2";

interface Request extends PromptOptions {
  kind: Kind;
}

export function DialogProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<Request | null>(null);
  const [value, setValue] = useState("");
  const resolver = useRef<((result: unknown) => void) | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const open = useCallback((req: Request): Promise<unknown> => {
    setValue(req.defaultValue ?? "");
    setRequest(req);
    return new Promise((resolve) => {
      resolver.current = resolve;
    });
  }, []);

  const settle = useCallback((result: unknown) => {
    resolver.current?.(result);
    resolver.current = null;
    setRequest(null);
    setValue("");
  }, []);

  const api = useMemo<Dialogs>(
    () => ({
      confirm: (o) => open({ kind: "confirm", ...o }) as Promise<boolean>,
      prompt: (o) => open({ kind: "prompt", ...o }) as Promise<string | null>,
      alert: (o) => open({ kind: "alert", ...o }).then(() => undefined),
    }),
    [open],
  );

  // Focus the field (prompt) so the user can type immediately.
  useEffect(() => {
    if (request?.kind === "prompt") {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [request]);

  if (request === null) {
    return (
      <DialogContext.Provider value={api}>{children}</DialogContext.Provider>
    );
  }

  const cancel = () => settle(request.kind === "prompt" ? null : false);
  const accept = () =>
    settle(
      request.kind === "prompt"
        ? value
        : request.kind === "alert"
          ? undefined
          : true,
    );

  return (
    <DialogContext.Provider value={api}>
      {children}
      <div className={SCRIM} role="presentation" onMouseDown={cancel}>
        <div
          className={PANEL}
          role="dialog"
          aria-modal="true"
          aria-label={request.title ?? request.message}
          onMouseDown={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === "Escape") cancel();
            else if (e.key === "Enter" && request.kind !== "prompt") accept();
          }}
        >
          {request.title !== undefined && (
            <h2 className={TITLE}>{request.title}</h2>
          )}
          <p className={MESSAGE}>{request.message}</p>
          {request.kind === "prompt" && (
            <input
              ref={inputRef}
              className={FIELD}
              name="dialog-value"
              autoComplete="off"
              aria-label={request.title ?? request.message}
              value={value}
              placeholder={request.placeholder}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") accept();
              }}
            />
          )}
          <div className={ACTIONS}>
            {request.kind !== "alert" && (
              <Button variant="ghost" onClick={cancel}>
                {request.cancelLabel ?? strings.dialogCancel}
              </Button>
            )}
            <Button
              variant={request.danger === true ? "danger" : "primary"}
              onClick={accept}
            >
              {request.confirmLabel ??
                (request.kind === "alert"
                  ? strings.dialogOk
                  : strings.dialogConfirm)}
            </Button>
          </div>
        </div>
      </div>
    </DialogContext.Provider>
  );
}
