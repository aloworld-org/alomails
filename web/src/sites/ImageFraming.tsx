// Framing a photograph: the crop rectangle and the focal point, drawn on the
// picture itself rather than typed as four numbers nobody can picture.
//
// Two ways in, always both (`docs/design/ux-principles.md`): drag on the
// image, or move the frame and the marker with the arrow keys — and the exact
// percentages stay visible and editable, because "a bit more off the left" is
// a drag and "the same crop as the other three photos" is a number.
//
// The control is fully controlled by the section draft: every gesture ends in
// an `onChange` carrying an image the store would accept. It never writes,
// and it never touches anything but `crop` and `focal`.
import { useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from "react";
import { Crosshair, Maximize2 } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import {
  FULL_BP,
  NUDGE_BP,
  asPercent,
  centerOf,
  clampFocal,
  cropBetween,
  cropOf,
  focalOf,
  framedImage,
  fromFraction,
  isFullFrame,
  moveCrop,
  setCropEdge,
  toPercent,
} from "./imageGeometry";
import type { CropEdge } from "./imageGeometry";
import { InformationTip } from "./parts";
import type { ImageCrop, SectionImage } from "./sections";
import styles from "./SitesModule.module.css";

/** Which corner a resize drag is holding; the opposite corner stays put. */
type Corner = "nw" | "ne" | "sw" | "se";

type Drag =
  | { kind: "new"; from: { x_bp: number; y_bp: number } }
  | { kind: "move"; grab: { x_bp: number; y_bp: number }; start: ImageCrop }
  | { kind: "corner"; anchor: { x_bp: number; y_bp: number } }
  | { kind: "focal" };

/** Each grab handle's own placement class; the keys are also the corners the
 *  control offers, so the list and the styling can never disagree. */
const HANDLE_STYLE = {
  nw: styles.framingHandleNw,
  ne: styles.framingHandleNe,
  sw: styles.framingHandleSw,
  se: styles.framingHandleSe,
};

const oppositeCorner = (crop: ImageCrop, corner: Corner): { x_bp: number; y_bp: number } => ({
  x_bp: corner === "nw" || corner === "sw" ? crop.x_bp + crop.width_bp : crop.x_bp,
  y_bp: corner === "nw" || corner === "ne" ? crop.y_bp + crop.height_bp : crop.y_bp,
});

/**
 * The framing control. `url` is the source image; while it is absent the
 * numbers still work, so a picture that fails to load costs the preview and
 * nothing else.
 */
export function ImageFraming({
  value,
  url,
  onChange,
}: {
  value: SectionImage;
  url: string | null;
  onChange: (patch: Partial<SectionImage>) => void;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const crop = cropOf(value);
  const focal = focalOf(value);
  const focalSet = value.focal !== undefined;

  /** Writes a frame back to the draft, keeping the two halves consistent. */
  function apply(next: ImageCrop, point = focal, set = focalSet) {
    const framed = framedImage(value, next, point, set);
    onChange({ crop: framed.crop, focal: framed.focal });
  }

  /** Where a pointer is, as a point of the source picture. */
  function pointAt(event: ReactPointerEvent): { x_bp: number; y_bp: number } {
    const box = surface.current?.getBoundingClientRect();
    if (box === undefined || box.width === 0 || box.height === 0) {
      return { x_bp: 0, y_bp: 0 };
    }
    return fromFraction((event.clientX - box.left) / box.width, (event.clientY - box.top) / box.height);
  }

  function start(event: ReactPointerEvent, next: Drag) {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrag(next);
  }

  function onPointerMove(event: ReactPointerEvent) {
    if (drag === null) return;
    const at = pointAt(event);
    switch (drag.kind) {
      case "new":
        apply(cropBetween(drag.from, at));
        break;
      case "move":
        apply(moveCrop(drag.start, at.x_bp - drag.grab.x_bp, at.y_bp - drag.grab.y_bp));
        break;
      case "corner":
        apply(cropBetween(drag.anchor, at));
        break;
      case "focal":
        apply(crop, clampFocal({ x_bp: at.x_bp, y_bp: at.y_bp }, crop), true);
        break;
    }
  }

  const end = (event: ReactPointerEvent) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDrag(null);
  };

  /** Arrow keys move the frame; with shift they resize it. */
  function onFrameKey(event: KeyboardEvent<HTMLDivElement>) {
    const nudges: Record<string, [number, number]> = {
      ArrowLeft: [-NUDGE_BP, 0],
      ArrowRight: [NUDGE_BP, 0],
      ArrowUp: [0, -NUDGE_BP],
      ArrowDown: [0, NUDGE_BP],
    };
    const nudge = nudges[event.key];
    if (nudge === undefined) return;
    event.preventDefault();
    const [dx, dy] = nudge;
    if (event.shiftKey) {
      apply({
        ...crop,
        width_bp: Math.min(FULL_BP - crop.x_bp, Math.max(0, crop.width_bp + dx)),
        height_bp: Math.min(FULL_BP - crop.y_bp, Math.max(0, crop.height_bp + dy)),
      });
    } else {
      apply(moveCrop(crop, dx, dy));
    }
  }

  function onFocalKey(event: KeyboardEvent<HTMLDivElement>) {
    const nudges: Record<string, [number, number]> = {
      ArrowLeft: [-NUDGE_BP, 0],
      ArrowRight: [NUDGE_BP, 0],
      ArrowUp: [0, -NUDGE_BP],
      ArrowDown: [0, NUDGE_BP],
    };
    const nudge = nudges[event.key];
    if (nudge === undefined) return;
    event.preventDefault();
    const [dx, dy] = nudge;
    apply(crop, clampFocal({ x_bp: focal.x_bp + dx, y_bp: focal.y_bp + dy }, crop), true);
  }

  const frameLabel = strings.sitesImageFrameAt(
    toPercent(crop.width_bp),
    toPercent(crop.height_bp),
    toPercent(crop.x_bp),
    toPercent(crop.y_bp),
  );
  const focalLabel = strings.sitesImageFocalAt(toPercent(focal.x_bp), toPercent(focal.y_bp));
  const stateLabel = `${isFullFrame(crop) ? strings.sitesImageWholePictureState : frameLabel}${
    focalSet ? ` · ${focalLabel}` : ""
  }`;

  return (
    <div className={styles.framing}>
      <div className={styles.framingHeader}>
        <div>
          <h4>{strings.sitesImageFraming}</h4>
          <p aria-live="polite">{stateLabel}</p>
        </div>
        <InformationTip
          label={strings.sitesImageFraming}
          hint={`${strings.sitesImageFrameHint} ${strings.sitesImageFocalHint}`}
        />
      </div>
      {url === null ? (
        <p className={styles.hint} role="status">
          {strings.sitesImageNoPreview}
        </p>
      ) : (
        <div
          ref={surface}
          className={styles.framingSurface}
          onPointerDown={(event) => start(event, { kind: "new", from: pointAt(event) })}
          onPointerMove={onPointerMove}
          onPointerUp={end}
          onPointerCancel={end}
        >
          <img className={styles.framingImage} src={url} alt="" draggable={false} />
          <div
            className={styles.framingFrame}
            style={{
              left: asPercent(crop.x_bp),
              top: asPercent(crop.y_bp),
              width: asPercent(crop.width_bp),
              height: asPercent(crop.height_bp),
            }}
            role="button"
            tabIndex={0}
            aria-label={frameLabel}
            onKeyDown={onFrameKey}
            onPointerDown={(event) =>
              start(event, { kind: "move", grab: pointAt(event), start: crop })
            }
            onPointerMove={onPointerMove}
            onPointerUp={end}
            onPointerCancel={end}
          >
            {(Object.keys(HANDLE_STYLE) as Corner[]).map((corner) => (
              <span
                key={corner}
                className={`${styles.framingHandle} ${HANDLE_STYLE[corner]}`}
                onPointerDown={(event) =>
                  start(event, { kind: "corner", anchor: oppositeCorner(crop, corner) })
                }
                onPointerMove={onPointerMove}
                onPointerUp={end}
                onPointerCancel={end}
              />
            ))}
          </div>
          <div
            className={styles.framingFocal}
            style={{ left: asPercent(focal.x_bp), top: asPercent(focal.y_bp) }}
            role="button"
            tabIndex={0}
            aria-label={focalLabel}
            onKeyDown={onFocalKey}
            onPointerDown={(event) => start(event, { kind: "focal" })}
            onPointerMove={onPointerMove}
            onPointerUp={end}
            onPointerCancel={end}
          />
        </div>
      )}
      <div className={styles.framingNumbers}>
        {(
          [
            ["width_bp", strings.sitesImageFrameWidth],
            ["height_bp", strings.sitesImageFrameHeight],
            ["x_bp", strings.sitesImageFrameLeft],
            ["y_bp", strings.sitesImageFrameTop],
          ] as [CropEdge, string][]
        ).map(([edge, label]) => (
          <label key={edge} className={styles.framingNumber}>
            <span>{label}</span>
            <span className={styles.framingNumberControl}>
              <input
                className={styles.input}
                type="number"
                min={0}
                max={100}
                aria-label={label}
                value={toPercent(crop[edge])}
                onChange={(event) => apply(setCropEdge(crop, edge, event.target.valueAsNumber))}
              />
              <span aria-hidden="true">%</span>
            </span>
          </label>
        ))}
      </div>
      <div className={styles.framingActions}>
        <Button
          variant="ghost"
          size="sm"
          icon={<Maximize2 />}
          disabled={isFullFrame(crop) && !focalSet}
          onClick={() => onChange({ crop: undefined, focal: undefined })}
        >
          {strings.sitesImageWholePicture}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          icon={<Crosshair />}
          disabled={!focalSet}
          onClick={() => apply(crop, centerOf(crop), false)}
        >
          {strings.sitesImageCentreFocal}
        </Button>
      </div>
    </div>
  );
}
