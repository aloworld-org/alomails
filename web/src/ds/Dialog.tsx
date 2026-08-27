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
import { AlertTriangle } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "./Button";
import { Input } from "./Input";
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
  "w-full max-w-[30rem] overflow-hidden border border-subtle " +
  "bg-surface rounded-2xl shadow-lg " +
  "animate-[alo-dialog-panel_var(--animation-dialog-panel)]";

const CONTENT = "flex gap-4 px-7 pb-7 pt-7";
const COPY = "min-w-0 flex-1";
const DANGER_ICON =
  "flex size-11 shrink-0 items-center justify-center rounded-xl " +
  "bg-danger-tint text-danger [&_svg]:size-5";
const TITLE = "m-0 text-lg font-semibold leading-6 text-primary";
/** `whitespace-pre-wrap`, because callers pass messages with their own line
 *  breaks and a confirm that reflows them reads as one run-on sentence. */
const MESSAGE =
  "mt-3 text-secondary text-md leading-7 whitespace-pre-wrap";

/** The one thing the prompt's field adds to `ds/Input`: room under the message.
 *
 *  It was a hand-rolled `.input` until the D1.55 wave check — a second one
 *  inside `ds/` itself, which is the exact duplication this design system
 *  exists to end, and it was kept only because D1.52 was contracted not to
 *  change how anything looked. Adopting `Input` changes two things and they are
 *  both the point: the field sits on the panel's own surface rather than on the
 *  app ground, and it shows focus as the outline every other control in `ds/`
 *  shows. It is also 40px tall now, like every other field in the product. */
const FIELD = "mt-5";

const ACTIONS =
  "flex items-center justify-end gap-3 border-t border-subtle bg-app/45 px-7 py-5";

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
            else if (
              e.key === "Enter" &&
              request.kind !== "prompt" &&
              !(e.target as HTMLElement).closest("button")
            )
              accept();
          }}
        >
          <div className={CONTENT}>
            {request.danger === true && (
              <span className={DANGER_ICON} aria-hidden="true">
                <AlertTriangle />
              </span>
            )}
            <div className={COPY}>
              {request.title !== undefined && (
                <h2 className={TITLE}>{request.title}</h2>
              )}
              <p className={MESSAGE}>{request.message}</p>
              {request.kind === "prompt" && (
                <Input
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
            </div>
          </div>
          <div className={ACTIONS}>
            {request.kind !== "alert" && (
              <Button
                variant="ghost"
                autoFocus={request.danger === true}
                onClick={cancel}
              >
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
