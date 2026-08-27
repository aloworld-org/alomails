import { useId, type ReactNode } from "react";

import { cx } from "../../ds";

interface HeaderFieldProps {
  id?: string;
  label: string;
  icon?: ReactNode;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}

export function HeaderField({
  id,
  label,
  icon,
  value,
  placeholder,
  onChange,
}: HeaderFieldProps) {
  const generatedId = useId();
  const fieldId = id ?? generatedId;
  return (
    <label className="grid gap-2" htmlFor={fieldId}>
      <span className="text-sm font-semibold text-primary">{label}</span>
      <span className="relative block">
        <input
          id={fieldId}
          className={cx(
            "h-12 w-full rounded-xl border border-default bg-surface px-4 text-base text-primary placeholder:text-tertiary hover:border-accent focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10",
            icon !== undefined && icon !== null && "pr-12",
          )}
          value={value}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
        />
        {icon && (
          <span className="pointer-events-none absolute inset-y-0 right-4 flex items-center text-secondary [&_svg]:size-5">
            {icon}
          </span>
        )}
      </span>
    </label>
  );
}
