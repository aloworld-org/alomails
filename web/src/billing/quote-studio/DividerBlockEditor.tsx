import { Minus, Pencil } from "lucide-react";
import { Button, ColorPicker, Modal, cx } from "../../ds";
import { strings } from "../../i18n";
import { DividerLine } from "./DividerLine";
import { DividerVisualChoice } from "./DividerVisualChoice";
import type { DividerBlock, DividerStyle, DividerThickness, DividerWidth } from "./QuoteStudioBlock";

const dividerWidthPreviewHeightClasses: Record<DividerWidth, string> = { 25: "h-px", 50: "h-0.5", 75: "h-[3px]", 100: "h-1" };

export function DividerBlockEditor({
  block,
  fallbackColor,
  onChange,
  open,
  onOpenChange,
}: {
  block: DividerBlock;
  fallbackColor: string;
  onChange: (patch: Partial<DividerBlock>) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const thickness = block.thickness ?? "fine";
  const style = block.style ?? "solid";
  const width = block.width ?? 100;
  const color = block.color ?? fallbackColor;
  const thicknessChoices: Array<{ value: DividerThickness; label: string }> = [
    { value: "fine", label: strings.quoteStudioDividerFine },
    { value: "medium", label: strings.quoteStudioDividerMedium },
    { value: "bold", label: strings.quoteStudioDividerBold },
  ];
  const styleChoices: Array<{ value: DividerStyle; label: string }> = [
    { value: "solid", label: strings.quoteStudioDividerSolid },
    { value: "dashed", label: strings.quoteStudioDividerDashed },
    { value: "dotted", label: strings.quoteStudioDividerDotted },
  ];
  const widthChoices: DividerWidth[] = [25, 50, 75, 100];

  return (
    <>
      <div className="flex min-h-16 items-center px-4 py-6">
        <DividerLine block={{ ...block, thickness, style, width, color }} />
      </div>
      {open && (
        <Modal
          title={strings.quoteStudioDividerSettings}
          icon={<Minus className="size-5" />}
          onClose={() => onOpenChange(false)}
          wide
          footer={
            <>
              <p className="mr-auto text-xs text-secondary">
                {strings.quoteStudioChangesImmediate}
              </p>
              <Button onClick={() => onOpenChange(false)}>
                {strings.quoteStudioDone}
              </Button>
            </>
          }
        >
          <div className="border-b border-subtle pb-5">
            <h3 className="text-base font-semibold text-primary">
              {strings.quoteStudioDividerAppearance}
            </h3>
            <p className="mt-1 text-sm text-secondary">
              {strings.quoteStudioDividerAppearanceHelp}
            </p>
          </div>

          <fieldset className="mt-1">
            <legend className="text-sm font-semibold text-primary">
              {strings.quoteStudioDividerStyle}
            </legend>
            <p className="mt-1 text-sm text-secondary">
              {strings.quoteStudioDividerStyleHelp}
            </p>
            <div className="mt-5 grid grid-cols-3 gap-5">
              {styleChoices.map((choice) => (
                <DividerVisualChoice
                  key={choice.value}
                  label={choice.label}
                  selected={choice.value === style}
                  onClick={() => onChange({ style: choice.value })}
                >
                  <DividerLine
                    block={{ ...block, style: choice.value, width: 100, color }}
                  />
                </DividerVisualChoice>
              ))}
            </div>
          </fieldset>

          <div className="mt-2 grid border-b border-subtle pb-7 md:grid-cols-2 md:divide-x md:divide-default">
            <fieldset className="pb-7 md:pb-0 md:pr-6">
              <legend className="text-sm font-semibold text-primary">
                {strings.quoteStudioDividerThickness}
              </legend>
              <p className="mt-1 text-sm text-secondary">
                {strings.quoteStudioDividerThicknessHelp}
              </p>
              <div className="mt-5 grid grid-cols-3 gap-3">
                {thicknessChoices.map((choice) => (
                  <DividerVisualChoice
                    key={choice.value}
                    label={choice.label}
                    selected={choice.value === thickness}
                    compact
                    onClick={() => onChange({ thickness: choice.value })}
                  >
                    <DividerLine
                      block={{
                        ...block,
                        thickness: choice.value,
                        width: 100,
                        color,
                      }}
                    />
                  </DividerVisualChoice>
                ))}
              </div>
            </fieldset>

            <fieldset className="border-t border-subtle pt-7 md:border-t-0 md:pl-6 md:pt-0">
              <legend className="text-sm font-semibold text-primary">
                {strings.quoteStudioDividerWidth}
              </legend>
              <p className="mt-1 text-sm text-secondary">
                {strings.quoteStudioDividerWidthHelp}
              </p>
              <div className="mt-5 grid grid-cols-4 gap-3">
                {widthChoices.map((choice) => (
                  <DividerVisualChoice
                    key={choice}
                    label={`${choice}%`}
                    selected={choice === width}
                    compact
                    onClick={() => onChange({ width: choice })}
                  >
                    <span
                      aria-hidden="true"
                      className={cx(
                        "block w-full rounded-full",
                        dividerWidthPreviewHeightClasses[choice],
                      )}
                      style={{ backgroundColor: color }}
                    />
                  </DividerVisualChoice>
                ))}
              </div>
            </fieldset>
          </div>

          <div className="mt-1">
            <p className="text-sm font-semibold text-primary">
              {strings.quoteStudioDividerColour}
            </p>
            <div className="mt-3 flex w-full items-center gap-4 rounded-xl border border-default bg-surface px-4 py-3.5">
              <span
                className="size-12 shrink-0 rounded-xl border border-black/10"
                style={{ backgroundColor: color }}
                aria-hidden="true"
              />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium text-primary">
                  {strings.quoteStudioDividerColour}
                </span>
                <span className="mt-1 block font-mono text-xs uppercase text-secondary">
                  {color}
                </span>
              </span>
              <ColorPicker
                label={strings.quoteStudioChooseDividerColour}
                value={color}
                onChange={(next) => onChange({ color: next })}
                triggerIcon={<Pencil className="size-4" />}
                triggerClassName="!size-10 !rounded-xl"
              />
            </div>
          </div>
        </Modal>
      )}
    </>
  );
}
