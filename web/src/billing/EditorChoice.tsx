import { cx } from "../ds";

export function EditorChoice<T extends string>({
  label,
  value,
  choices,
  onChange,
}: {
  label: string;
  value: T;
  choices: ReadonlyArray<readonly [T, string]>;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset>
      <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</legend>
      <div className="grid grid-cols-2 gap-2">
        {choices.map(([id, name]) => (
          <button
            key={id}
            type="button"
            className={cx(
              "min-h-10 rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
              value === id
                ? "border-accent bg-accent-soft text-accent"
                : "border-default bg-surface text-primary hover:border-accent/50 hover:bg-raised",
            )}
            aria-pressed={value === id}
            onClick={() => onChange(id)}
          >
            {name}
          </button>
        ))}
      </div>
    </fieldset>
  );
}
