// The lifecycle actions on a billing document — issue, void, credit, send,
// accept, decline, expire — and the confirmation each one asks for first.
//
// Every action here is **irreversible on a legal document**: issuing spends a
// number out of a gapless series, sending freezes an offer, and answering one
// closes it for good. So none of them is a bare button. Each states, in the
// confirm dialog, what it will do to the document — not "are you sure" — and
// the destructive ones are styled as such.
//
// The component owns only the asking, the busy state and the reporting. What
// an action *does* is the editor's, because only the editor knows whether the
// answer is a document to adopt or another screen to go to.
import { useState } from "react";

import { Button, useDialogs } from "../ds";
import { strings } from "../i18n";
import { billingMessage } from "./api";
import styles from "./billingStyles";

/** One thing that can be done to the document at its current state. */
export interface DocumentAction {
  /** Stable identity; also what a test finds the button by. */
  key: string;
  label: string;
  /** The dialog shown before it runs: what this does to the document. */
  title: string;
  message: string;
  /** Styled as destructive — voiding, declining. */
  danger?: boolean;
  /** The action the state's normal next step (at most one per state). */
  primary?: boolean;
  run: () => Promise<void>;
}

/**
 * The action bar of a document. Renders nothing at all when the document is in
 * a state that offers no transitions — a void invoice, a declined offer — so a
 * closed document has no buttons rather than disabled ones.
 */
export function DocumentActions({
  actions,
  unsaved,
  onFailed,
}: {
  actions: DocumentAction[];
  /**
   * The form holds edits the server has not stored yet.
   *
   * Every transition acts on the **stored** document, so firing one now would
   * freeze a document that is not the one on screen — the keystrokes since the
   * last save would vanish into a document nobody can edit any more. So the
   * actions wait, and say what they are waiting for. A row that cannot become
   * a line keeps this true indefinitely, which is right: a document whose
   * editor holds an unsendable line is not a document to issue.
   */
  unsaved: boolean;
  /** Where a refusal goes: the editor's error banner, in the server's words. */
  onFailed: (message: string) => void;
}) {
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState<string | null>(null);

  if (actions.length === 0) return null;

  async function invoke(action: DocumentAction) {
    if (
      !(await confirm({
        title: action.title,
        message: action.message,
        confirmLabel: action.label,
        danger: action.danger ?? false,
      }))
    ) {
      return;
    }
    setBusy(action.key);
    try {
      await action.run();
    } catch (err) {
      onFailed(billingMessage(err, strings.billingActionFailed));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className={styles.actionBar}>
      {unsaved && <p className={styles.hint}>{strings.billingActionsWaitForSave}</p>}
      {actions.map((action) => (
        <Button
          key={action.key}
          variant={
            action.primary === true ? "primary" : action.danger === true ? "danger" : "ghost"
          }
          disabled={busy !== null || unsaved}
          onClick={() => void invoke(action)}
        >
          {action.label}
        </Button>
      ))}
    </div>
  );
}
