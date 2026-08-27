import type { LucideIcon } from "lucide-react";

export function AddButton({
  label,
  help,
  Icon,
  disabled = false,
  onClick,
}: {
  label: string;
  help: string;
  Icon: LucideIcon;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      className="flex min-h-16 items-center gap-3 rounded-xl px-3 py-2.5 text-left text-primary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-not-allowed disabled:opacity-45"
      onClick={onClick}
    >
      <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
        <Icon className="size-5" aria-hidden="true" />
      </span>
      <span>
        <span className="block text-sm font-semibold">{label}</span>
        <span className="mt-0.5 block text-xs text-secondary">{help}</span>
      </span>
    </button>
  );
}
