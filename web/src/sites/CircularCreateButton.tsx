import { Plus } from "lucide-react";

export function CircularCreateButton({
  label,
  disabled = false,
  expanded,
  sectionComposer = false,
  onClick,
}: {
  label: string;
  disabled?: boolean;
  expanded?: boolean;
  sectionComposer?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="grid size-10 shrink-0 place-items-center rounded-full !bg-accent-soft !p-0 !text-accent transition-colors hover:!bg-accent hover:!text-on-accent focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15 disabled:cursor-not-allowed disabled:opacity-50"
      aria-label={label}
      title={label}
      {...(sectionComposer ? { "data-add-section": "" } : {})}
      {...(expanded === undefined ? {} : { "aria-expanded": expanded })}
      disabled={disabled}
      onClick={onClick}
    >
      <Plus className="size-4" aria-hidden="true" />
      <span className="sr-only">{label}</span>
    </button>
  );
}
