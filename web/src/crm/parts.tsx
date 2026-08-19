// The pieces the CRM screens share, so the board, the list, the drawer and the
// deal form are visibly one module. Presentational only: no data loading, no
// rules, no arithmetic.
//
// Since D2.07 the primitives underneath are the design system's: the state
// word is a `ds/Badge`, and the dialog frame is a `ds/Modal` rather than this
// module's own scrim and panel. The module's own `Field` is gone entirely —
// `ds/Field` binds the label to the control and announces the error, which is
// the part a hand-rolled column never had.
import { useId, type FormEvent, type ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Badge, Button, IconButton, Modal } from "../ds";
import { strings } from "../i18n";
import { stateLabel } from "./format";
import type { DealState } from "./types";
import styles from "./CrmModule.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one (a board a user fills has one; a filtered list that matched nothing does
 *  not — offering a button there would invent an action). */
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

/** Where a deal stands, as the server derived it. A `Badge` rather than a
 *  `Chip`: the design system's line is that a badge is read and a chip is acted
 *  on, and nothing about this word is pressable. The name stays `StateChip`
 *  because that is what two screens call it.
 *
 *  The tone is never the only signal — the word itself says "Won" or "Lost" —
 *  which is why an open deal takes the accent tone rather than needing a fifth
 *  one of its own. */
export function StateChip({ state }: { state: DealState }) {
  const tone =
    state === "won" ? "success" : state === "lost" ? "danger" : "accent";
  return <Badge tone={tone}>{stateLabel(state)}</Badge>;
}

/** The modal chrome the deal form sits in: a `ds/Modal` whose body is the form
 *  and whose footer submits it.
 *
 *  Body and footer are siblings inside the panel, so the submit button is tied
 *  to the form by id rather than nested in it — which is also what keeps Enter
 *  in any field working.
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
          label={strings.crmCancel}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.crmCancel}
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
