// The pieces the HR screens share, so the board, the drawer, the tables and the
// five forms are visibly one module. Presentational only: no data loading, no
// rules, no dates computed, no access decided.
//
// Since D2.08b the primitives underneath are the design system's: the state
// word is a `ds/Badge`, and the dialog frame is a `ds/Modal` rather than this
// module's own scrim and panel. The module's own `Field` is gone entirely —
// `ds/Field` binds the label to the control and announces the error, which is
// the part a hand-rolled column never had: this one put the words *beside* the
// box without ever telling a screen reader they belonged to it.
import { useId, type FormEvent, type ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Badge, Button, IconButton, Modal } from "../ds";
import { strings } from "../i18n";
import styles from "./hr.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one. A hiring board with no roles on it has one; a closed round with nobody
 *  in a column does not, because there is nothing to add there. */
export function EmptyState({
  Icon,
  title,
  body,
  cta,
  onCta,
}: {
  Icon: LucideIcon;
  title: string;
  body: string;
  cta?: string;
  onCta?: () => void;
}) {
  return (
    <div className={styles.empty}>
      <span className={styles.emptyArt} aria-hidden="true">
        <Icon size={38} />
      </span>
      <h2 className={styles.emptyTitle}>{title}</h2>
      <p className={styles.emptyBody}>{body}</p>
      {cta !== undefined && onCta !== undefined && (
        <Button onClick={onCta}>{cta}</Button>
      )}
    </div>
  );
}

/** The tone of a state the server decided, in this module's own words: a
 *  neutral fact, a good outcome, a bad one. It stays the module's vocabulary
 *  rather than becoming the design system's tone names, because `leave.ts`
 *  maps eight leave states onto it and that mapping is tested. */
export type StateTone = "info" | "good" | "bad";

/** A small coloured word for a state the server decided — hired, refused,
 *  waiting, left, past its retention date.
 *
 *  A `ds/Badge` rather than a `ds/Chip`: the design system's line is that a
 *  badge is read and a chip is acted on, and not one of these is pressable.
 *  The two tints this replaced were hardcoded (`#e3f3e9` on `#1f6b45`,
 *  `#fbe6e2` on `#9b3222`) and are the success and danger tokens now, so a
 *  refused request in HR is the same red as a refused claim in Finance.
 *
 *  The tone is never the only signal — the word itself says which state it
 *  is — which is why `info` takes the accent tone rather than needing one of
 *  its own. */
export function StateBadge({
  tone,
  children,
}: {
  tone: StateTone;
  children: ReactNode;
}) {
  return (
    <Badge
      tone={tone === "good" ? "success" : tone === "bad" ? "danger" : "accent"}
    >
      {children}
    </Badge>
  );
}

/** The modal chrome the five HR forms sit in: a `ds/Modal` whose body is the
 *  form and whose footer submits it.
 *
 *  Body and footer are siblings inside the panel, so the submit button is tied
 *  to the form by id rather than nested in it — which is also what keeps Enter
 *  in any field working.
 *
 *  What the panel this replaced never had: a focus trap, and an Escape that
 *  works before you have clicked into the dialog. Its `onKeyDown` sat on the
 *  form, so the key only closed a form somebody was already inside, and Tab
 *  walked straight out onto the board behind it.
 *
 *  The header carries the close button and the footer carries Cancel, both
 *  named "Cancel": two ways out of the same question, said the same way. */
export function DialogFrame({
  Icon,
  title,
  subtitle,
  error,
  busy,
  canSubmit,
  submitLabel,
  onClose,
  onSubmit,
  children,
}: {
  Icon: LucideIcon;
  title: string;
  subtitle: string;
  error: string | null;
  busy: boolean;
  canSubmit: boolean;
  submitLabel: string;
  onClose: () => void;
  onSubmit: () => void;
  children: ReactNode;
}) {
  const formId = useId();
  function submit(e: FormEvent) {
    e.preventDefault();
    if (!busy && canSubmit) onSubmit();
  }
  return (
    <Modal
      title={title}
      onClose={onClose}
      icon={<Icon size={19} />}
      actions={
        <IconButton
          label={strings.hrCancel}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.hrCancel}
          </Button>
          <Button type="submit" form={formId} disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </>
      }
    >
      {/* The sentence under the title. `ds/Modal`'s header is the name and the
          controls, so the question this dialog is asking reads as the first
          line of the body rather than as a second heading. */}
      <p className="m-0 text-sm text-tertiary">{subtitle}</p>
      {error !== null && <ErrorBanner message={error} />}
      <form id={formId} className="flex flex-col gap-4" onSubmit={submit}>
        {children}
      </form>
    </Modal>
  );
}
