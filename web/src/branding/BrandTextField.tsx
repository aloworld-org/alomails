import { Field, Input } from "../ds";

export function BrandTextField({
  label,
  hint,
  placeholder,
  value,
  maximum,
  multiline = false,
  onChange,
}: {
  label: string;
  hint: string;
  placeholder: string;
  value: string;
  maximum: number;
  multiline?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      {(control) => multiline ? (
        <textarea
          id={control.id}
          aria-describedby={control["aria-describedby"]}
          className="min-h-28 w-full resize-y rounded-md border border-default bg-surface px-3 py-2 font-[inherit] text-base leading-6 text-primary placeholder:text-tertiary focus:border-accent focus:outline-none focus-visible:outline-none disabled:bg-raised disabled:text-tertiary"
          value={value}
          maxLength={maximum}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <Input
          id={control.id}
          aria-describedby={control["aria-describedby"]}
          value={value}
          maxLength={maximum}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </Field>
  );
}
