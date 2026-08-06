// Dialog context lives separately from the rendered provider on purpose.
// Keeping the context object in a context-only module preserves its identity
// when Vite Fast Refresh replaces Dialog.tsx during local development.
import { createContext, useContext } from "react";

export interface ConfirmOptions {
  title?: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Style the confirm action as destructive (used for deletes). */
  danger?: boolean;
}

export interface PromptOptions extends ConfirmOptions {
  placeholder?: string;
  defaultValue?: string;
}

export interface AlertOptions {
  title?: string;
  message: string;
  confirmLabel?: string;
}

export interface Dialogs {
  confirm(options: ConfirmOptions): Promise<boolean>;
  prompt(options: PromptOptions): Promise<string | null>;
  alert(options: AlertOptions): Promise<void>;
}

export const DialogContext = createContext<Dialogs | null>(null);

export function useDialogs(): Dialogs {
  const ctx = useContext(DialogContext);
  if (ctx === null) {
    throw new Error("useDialogs must be used within <DialogProvider>");
  }
  return ctx;
}
