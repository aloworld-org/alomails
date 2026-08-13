// The visible half of constrained resize (ADR 0042, S3.01c): one row of
// choices per resizable property of a section, showing what it is on now and
// what else it could be.
//
// Every button here comes from the server's own declaration — this component
// renders what it is given and invents nothing, so a value the store would
// refuse is a value the screen cannot show. That is also why the buttons are
// radios rather than a free control: recognition over recall
// (`docs/design/ux-principles.md`), and the whole point of ADR 0042 is that
// the answer space is small enough to *show*.
//
// The same change can be made on the page itself, with `Alt` and an arrow key
// on the focused section; both go through one function in the editor, so
// there is one door, one announcement and one undo entry.
import { strings } from "../i18n";
import { layoutControlLabel, layoutValueLabel } from "./sectionInfo";
import { controlsFor, currentValue, type SectionLayouts } from "./sectionLayout";
import type { Section } from "./sections";
import styles from "./SitesModule.module.css";

export interface SectionLayoutControlsProps {
  /** The section being resized. */
  section: Section;
  /** Its position in the stack — the coordinate the change names. */
  index: number;
  /** The server's declaration; a type absent from it renders nothing. */
  layouts: SectionLayouts;
  /** Whether the editor is busy or in a state where nothing may be written. */
  disabled: boolean;
  /** Apply: `key` names the control, `value` is one of its declared values. */
  onChoose: (index: number, key: string, value: string) => void;
}

/** The choice rows for one section, or nothing at all when its type declares
 *  no resizable property. */
export function SectionLayoutControls({
  section,
  index,
  layouts,
  disabled,
  onChoose,
}: SectionLayoutControlsProps) {
  const controls = controlsFor(layouts, section);
  if (controls.length === 0) return null;
  return (
    <div className={styles.layoutControls}>
      {controls.map((control) => {
        const on = currentValue(section, control);
        return (
          <div
            key={control.key}
            className={styles.layoutControl}
            role="radiogroup"
            aria-label={strings.sitesLayoutOf(layoutControlLabel(control.key))}
          >
            <span className={styles.layoutControlName}>
              {layoutControlLabel(control.key)}
            </span>
            {control.values.map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={value === on}
                disabled={disabled}
                data-layout-choice={`${control.key}/${value}`}
                className={
                  value === on
                    ? `${styles.layoutChoice} ${styles.layoutChoiceActive}`
                    : styles.layoutChoice
                }
                onClick={() => onChoose(index, control.key, value)}
              >
                {layoutValueLabel(control.key, value)}
              </button>
            ))}
          </div>
        );
      })}
    </div>
  );
}
