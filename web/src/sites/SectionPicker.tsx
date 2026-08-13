// The add-section picker: a grid of the sixteen section types, each tile a
// small schematic thumbnail with the type's name and one line on what it is
// for. Choosing a tile hands the kind back — the prop form takes it from
// there; nothing is written until that form saves.
import { LayoutGrid, X } from "lucide-react";
import { useRef, type ReactNode } from "react";

import { strings } from "../i18n";
import { kindDescription, kindLabel } from "./sectionInfo";
import { SECTION_KINDS } from "./sections";
import type { SectionKind } from "./sections";
import { useDialogKeyboard } from "./useDialogKeyboard";
import styles from "./SitesModule.module.css";

/** The schematic thumbnails, one per kind: not screenshots, just the shape a
 *  stranger recognizes the block by. Decorative — the tile's text names it. */
function thumbnail(kind: SectionKind): ReactNode {
  switch (kind) {
    case "nav":
      return (
        <>
          <rect x="2" y="4" width="60" height="8" rx="2" opacity="0.25" />
          <circle cx="8" cy="8" r="2.5" />
          <rect x="36" y="6.5" width="8" height="3" rx="1.5" />
          <rect x="47" y="5" width="13" height="6" rx="3" opacity="0.7" />
        </>
      );
    case "hero":
      return (
        <>
          <rect x="12" y="8" width="40" height="6" rx="2" />
          <rect x="18" y="18" width="28" height="3" rx="1.5" opacity="0.45" />
          <rect x="24" y="26" width="16" height="7" rx="3.5" opacity="0.7" />
        </>
      );
    case "features":
      return (
        <>
          {[4, 24, 44].map((x) => (
            <g key={x}>
              <circle cx={x + 8} cy="10" r="4" opacity="0.7" />
              <rect x={x} y="18" width="16" height="3" rx="1.5" />
              <rect x={x} y="24" width="16" height="2" rx="1" opacity="0.45" />
              <rect x={x} y="28" width="12" height="2" rx="1" opacity="0.45" />
            </g>
          ))}
        </>
      );
    case "text_image":
      return (
        <>
          <rect x="2" y="6" width="28" height="28" rx="2" opacity="0.35" />
          <circle cx="11" cy="16" r="4" opacity="0.7" />
          <path d="M4 30 L14 20 L20 26 L26 20 L30 24 L30 32 L4 32 Z" opacity="0.7" />
          <rect x="36" y="10" width="24" height="4" rx="2" />
          <rect x="36" y="18" width="24" height="2" rx="1" opacity="0.45" />
          <rect x="36" y="22" width="24" height="2" rx="1" opacity="0.45" />
          <rect x="36" y="26" width="16" height="2" rx="1" opacity="0.45" />
        </>
      );
    case "gallery":
      return (
        <>
          {[2, 23, 44].map((x) =>
            [4, 22].map((y) => (
              <rect key={`${x}-${y}`} x={x} y={y} width="18" height="14" rx="2" opacity="0.4" />
            )),
          )}
        </>
      );
    case "testimonials":
      return (
        <>
          <path d="M8 8 q-4 0 -4 5 v3 h6 v-6 q0 -2 -2 -2 Z" opacity="0.7" />
          <rect x="14" y="8" width="46" height="3" rx="1.5" opacity="0.45" />
          <rect x="14" y="14" width="38" height="3" rx="1.5" opacity="0.45" />
          <circle cx="10" cy="30" r="4" opacity="0.7" />
          <rect x="18" y="28" width="20" height="3" rx="1.5" />
        </>
      );
    case "pricing":
      return (
        <>
          <rect x="3" y="10" width="17" height="24" rx="2" opacity="0.3" />
          <rect x="23" y="5" width="18" height="30" rx="2" opacity="0.6" />
          <rect x="44" y="10" width="17" height="24" rx="2" opacity="0.3" />
        </>
      );
    case "team":
      return (
        <>
          {[10, 32, 54].map((x) => (
            <g key={x}>
              <circle cx={x} cy="12" r="6" opacity="0.6" />
              <rect x={x - 7} y="22" width="14" height="3" rx="1.5" />
              <rect x={x - 5} y="28" width="10" height="2" rx="1" opacity="0.45" />
            </g>
          ))}
        </>
      );
    case "faq":
      return (
        <>
          {[4, 16, 28].map((y) => (
            <g key={y}>
              <rect x="2" y={y} width="60" height="8" rx="2" opacity="0.25" />
              <rect x="6" y={y + 3} width="30" height="2" rx="1" />
              <path d={`M54 ${y + 2.5} l3 3 l3 -3`} fill="none" stroke="currentColor" />
            </g>
          ))}
        </>
      );
    case "cta":
      return (
        <>
          <rect x="2" y="10" width="60" height="20" rx="3" opacity="0.25" />
          <rect x="8" y="16" width="26" height="4" rx="2" />
          <rect x="42" y="15" width="14" height="7" rx="3.5" opacity="0.7" />
        </>
      );
    case "contact_form":
      return (
        <>
          <rect x="8" y="4" width="48" height="6" rx="2" opacity="0.35" />
          <rect x="8" y="13" width="48" height="6" rx="2" opacity="0.35" />
          <rect x="8" y="22" width="48" height="8" rx="2" opacity="0.35" />
          <rect x="40" y="33" width="16" height="5" rx="2.5" opacity="0.7" />
        </>
      );
    case "collection":
      return (
        <>
          {[3, 24, 45].map((x) => (
            <g key={x}>
              <rect x={x} y="6" width="16" height="28" rx="2" opacity="0.3" />
              <rect x={x + 3} y="10" width="10" height="8" rx="1" opacity="0.7" />
              <rect x={x + 3} y="22" width="10" height="2" rx="1" />
              <rect x={x + 3} y="27" width="7" height="2" rx="1" opacity="0.5" />
            </g>
          ))}
        </>
      );
    case "catalog":
      return (
        <>
          {[6, 22].map((y) => (
            <g key={y}>
              <rect x="4" y={y} width="12" height="12" rx="2" opacity="0.55" />
              <rect x="20" y={y + 2} width="22" height="3" rx="1.5" />
              <rect x="20" y={y + 8} width="14" height="2" rx="1" opacity="0.45" />
              <rect x="50" y={y + 3} width="10" height="4" rx="2" opacity="0.7" />
            </g>
          ))}
        </>
      );
    case "booking":
      return (
        <>
          <rect x="4" y="6" width="26" height="28" rx="2" opacity="0.3" />
          <rect x="8" y="10" width="18" height="3" rx="1.5" />
          {[16, 22, 28].map((y) => (
            <rect key={y} x="8" y={y} width="18" height="3" rx="1.5" opacity="0.5" />
          ))}
          <rect x="36" y="10" width="24" height="4" rx="2" opacity="0.7" />
          <rect x="36" y="19" width="24" height="3" rx="1.5" opacity="0.45" />
          <rect x="36" y="27" width="14" height="6" rx="3" opacity="0.7" />
        </>
      );
    case "custom_code":
      // A framed box with brackets in it: the block is code, and the frame
      // around it is the point rather than decoration.
      return (
        <>
          <rect
            x="6"
            y="6"
            width="52"
            height="28"
            rx="3"
            fill="none"
            stroke="currentColor"
            strokeDasharray="4 3"
            opacity="0.6"
          />
          <path
            d="M24 15 L18 20 L24 25 M40 15 L46 20 L40 25"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          />
          <rect x="30" y="13" width="4" height="14" rx="2" opacity="0.45" />
        </>
      );
    case "footer":
      return (
        <>
          <rect x="2" y="26" width="60" height="12" rx="2" opacity="0.25" />
          <rect x="6" y="31" width="16" height="2" rx="1" />
          <rect x="34" y="31" width="8" height="2" rx="1" opacity="0.45" />
          <rect x="45" y="31" width="8" height="2" rx="1" opacity="0.45" />
        </>
      );
  }
}

/** The picker dialog. Escape or the scrim closes it without choosing. */
export function SectionPicker({
  onPick,
  onClose,
}: {
  onPick: (kind: SectionKind) => void;
  onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  useDialogKeyboard(panel, onClose);
  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <div
        ref={panel}
        className={`${styles.modal} ${styles.pickerModal}`}
        role="dialog"
        aria-modal="true"
        aria-label={strings.sitesPickerTitle}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className={styles.modalHead}>
          <span className={styles.modalIcon} aria-hidden="true">
            <LayoutGrid size={19} />
          </span>
          <div className={styles.modalHeadText}>
            <h2>{strings.sitesPickerTitle}</h2>
            <p>{strings.sitesPickerSubtitle}</p>
          </div>
          <button
            type="button"
            className={styles.modalClose}
            onClick={onClose}
            aria-label={strings.close}
          >
            <X size={18} />
          </button>
        </div>
        <div className={`${styles.modalBody} ${styles.pickerGrid}`}>
          {SECTION_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              className={styles.pickerTile}
              onClick={() => onPick(kind)}
            >
              <svg
                className={styles.pickerThumb}
                viewBox="0 0 64 40"
                aria-hidden="true"
                focusable="false"
              >
                {thumbnail(kind)}
              </svg>
              <span className={styles.pickerTileName}>{kindLabel(kind)}</span>
              <span className={styles.pickerTileDesc}>{kindDescription(kind)}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
