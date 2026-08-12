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
import styles from "./Dialog.module.css";

type Kind = "confirm" | "prompt" | "alert";

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
      <div className={styles.scrim} role="presentation" onMouseDown={cancel}>
        <div
          className={styles.dialog}
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
            <h2 className={styles.title}>{request.title}</h2>
          )}
          <p className={styles.message}>{request.message}</p>
          {request.kind === "prompt" && (
            <input
              ref={inputRef}
              className={styles.input}
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
          <div className={styles.actions}>
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
