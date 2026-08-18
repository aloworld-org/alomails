// A setting that applies itself (ADR 0045). See `Checkbox` for the distinction.
//
// Seven stylesheets declared `.toggle` and they were not all the same object.
// Two — `admin` and `shell/SettingsModal` — drew a switch: a hidden checkbox,
// a rounded track, a knob that slides. The other five styled a row containing
// an ordinary checkbox, or in three cases a `<select>`, under the same name.
// That is the whole reason the word had stopped meaning anything, so it is
// split here and the split is the useful part:
//
//   **A toggle is a setting. A checkbox is one option among several.**
//
// A toggle takes effect the moment it moves and is announced "on" or "off";
// it sits on its own row, usually with a sentence under it saying what turning
// it on does. "Give this person admin access" is a toggle. A box beside a
// search field that narrows a list is a `Checkbox` — see that file.
//
// It stays a native `<input type="checkbox">` with `role="switch"`, for the
// same reason `Select` stays a native select: space-to-toggle, form
// participation, `:checked` and the label association are all already there,
// and `role` is the one thing the platform cannot infer.
//
// What the two copies were missing was never the geometry:
//
//   * **A name.** `admin/UsersPage` names its switch with `title` on the
//     wrapping `<label>` — a tooltip, not a name; the label's text content is
//     empty, so the control is announced as "checkbox, not checked" in a table
//     of twenty identical ones. `admin/UserModal` puts `aria-label` on the
//     `<label>` while the visible text sits in a sibling `<span>` that is bound
//     to nothing. `label` is required here and is really bound.
//   * **The state, said as a state.** Both were plain checkboxes, so "out of
//     office" was read as "checked" rather than "on".
//   * **The hint.** `shell/SettingsModal` renders a hint under every switch
//     and describes none of them; it is on screen and absent from the
//     announcement. `hint` is wired to `aria-describedby`.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The two copies that drew a switch — `admin` (38×22, 16px knob) and
// `shell/SettingsModal` (40×24, 18px knob) — were otherwise the same rule
// twice: a visually-hidden checkbox, a full-radius track, a `::after` knob,
// `translateX(16px)` when checked. The larger geometry is the base: 24px is the
// short side of the smallest target WCAG 2.5.8 accepts, and both copies already
// travelled the same 16px, so nothing else had to move.
//
// Reconciled rather than offered as options:
//
//   * **A focus ring.** Neither copy had one. The input is `opacity: 0`, so a
//     keyboard user tabbing down the admin user list — twenty switches, one per
//     row — got no indication of where they were (WCAG 2.4.7). The ring is
//     drawn on the track, since the control the eye sees is the track.
//   * **A visible off state.** Both filled the off track with `--border-strong`
//     (#cbd5e1), which is 1.7:1 against the surface it sits on — under the 3:1
//     that WCAG 1.4.11 asks of a control's own boundary, and the white knob was
//     1.7:1 against the track it sits in. `--text-tertiary` is the same quiet
//     role in a fill that it plays in type, and it is 4.8:1. The on track keeps
//     `--accent` (3.1:1 against the surface, 3.1:1 against the knob).
//   * **A disabled state.** Both disable the input on real conditions — an
//     unconfigured provider, your own admin row — and neither changed anything
//     you could see: the same track, the same pointer. A control that refuses
//     looked exactly like one that was broken. Dimmed, but not below the floor:
//     `--text-tertiary` on `--bg-surface` is 4.8:1, because a setting you are
//     not allowed to change still has to be readable.
//
// The knob's position is the primary signal and the colour is the second one,
// for the reason `Badge` states about tone: colour alone is invisible to
// somebody who cannot distinguish it, and off-grey and on-terracotta are only
// 1.6:1 apart. Motion is `--duration-fast`; the reduced-motion override in
// `global.css` already flattens it.
import { useId } from "react";

/** The switch's own geometry. The knob's numbers stay literal, as they were in
 *  the stylesheet this replaces: 3px of inset and an 18px knob inside a 40×24
 *  track are one drawing's proportions, not values any other component shares.
 *  40 − 18 − 3 − 3 = 16, which is the travel both copies had arrived at and is
 *  `--space-4`. */
const TRACK =
  "block w-full h-full rounded-full " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "after:content-[''] after:absolute after:top-[3px] after:left-[3px] " +
  "after:w-[18px] after:h-[18px] after:rounded-full " +
  // The knob is a surface lifted off the track, so it is the surface token
  // rather than the `#fff` both copies hardcoded into a file with 7,422 token
  // references.
  "after:bg-surface after:shadow-sm " +
  "after:transition-transform after:duration-[var(--duration-fast)] after:ease-standard " +
  "peer-checked:after:translate-x-4 " +
  "peer-focus-visible:outline-2 peer-focus-visible:outline-accent peer-focus-visible:outline-offset-2";

/** The track's two fills, chosen as a pair. Two utilities that set the same
 *  property have no defined winner, so "off or on" and "live or refusing" are
 *  resolved here rather than layered and hoped over. */
const TRACK_FILL = "bg-tertiary peer-checked:bg-accent";
const TRACK_FILL_DISABLED = "bg-strong peer-checked:bg-accent-tint";

export interface ToggleProps {
  /** Controlled: this component holds no state of its own. */
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** What the switch turns on, as a sentence a person could act on. Required:
   *  a switch has no value to fall back on and no content of its own, so an
   *  unnamed one is announced as nothing but its role. */
  label: string;
  /** Read but not drawn — for a switch in a table row whose column header
   *  already says what it does. Never a way to skip naming it. */
  hideLabel?: boolean | undefined;
  /** What turning it on actually does, when that is not obvious from the
   *  label. Described, so it is heard as well as seen. */
  hint?: string | undefined;
  /** `row` gives the label the width and puts the switch at the end of it —
   *  a settings list. The default keeps them side by side at their natural
   *  widths. */
  layout?: "inline" | "row" | undefined;
  disabled?: boolean | undefined;
  className?: string | undefined;
}

export function Toggle({
  checked,
  onChange,
  label,
  hideLabel,
  hint,
  layout = "inline",
  disabled,
  className,
}: ToggleProps) {
  const id = useId();
  const hintId = hint === undefined ? undefined : `${id}-hint`;

  const classes = [
    // A settings row gives the sentence the width and puts the switch at the
    // end of it, which is where `shell/SettingsModal` put it and where a row
    // with a hint under it needs it. The default keeps them side by side at
    // their natural widths.
    layout === "row" ? "flex w-full" : "inline-flex",
    "group items-center gap-3 has-[:disabled]:cursor-not-allowed",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <span className={classes}>
      {/* The positioning context for the input, which lies on top of the track
          so that a click anywhere on the switch hits the control rather than
          the label's `for`. Both copies did this and neither sized the input,
          so the hit area was a default-sized checkbox roughly over a 40px
          track. */}
      <span className="relative flex-none w-10 h-6">
        <input
          type="checkbox"
          // Allowed on a checkbox by ARIA in HTML, and the browser maps the
          // element's checkedness onto it — so the state stays the platform's
          // and only the wording changes.
          role="switch"
          id={id}
          className="peer absolute inset-0 w-full h-full m-0 opacity-0 cursor-pointer disabled:cursor-not-allowed"
          checked={checked}
          disabled={disabled === true}
          {...(hintId === undefined ? {} : { "aria-describedby": hintId })}
          onChange={(e) => onChange(e.target.checked)}
        />
        {/* The knob and the track are the drawing; the input above them is the
            control. Hidden from assistive technology so the switch is
            announced once. */}
        <span
          className={`${TRACK} ${disabled === true ? TRACK_FILL_DISABLED : TRACK_FILL}`}
          aria-hidden="true"
        />
      </span>
      <span
        className={`flex flex-col gap-1 min-w-0${layout === "row" ? " -order-1 flex-auto" : ""}`}
      >
        <label
          htmlFor={id}
          className={
            hideLabel === true
              ? // Read, not drawn — for a switch in a table row where the
                // column header already says what the switch does. The name
                // still has to exist: a screen reader in that column otherwise
                // hears "switch, off" twenty times.
                "sr-only"
              : "text-base text-primary cursor-pointer " +
                "group-has-[:disabled]:text-tertiary group-has-[:disabled]:cursor-not-allowed"
          }
        >
          {label}
        </label>
        {hint !== undefined && (
          <span
            className="text-sm text-secondary group-has-[:disabled]:text-tertiary"
            id={hintId}
          >
            {hint}
          </span>
        )}
      </span>
    </span>
  );
}
