import { strings } from "../i18n";

export function BrandColorBalance({ primary, secondary }: { primary: string; secondary: string }) {
  const roles = [
    { label: strings.brandingNeutral, ratio: "70%", color: "var(--bg-raised)" },
    { label: strings.brandingSecondary, ratio: "20%", color: secondary },
    { label: strings.brandingPrimary, ratio: "10%", color: primary },
  ];

  return (
    <section className="rounded-2xl border border-subtle bg-raised p-4 sm:p-5" aria-label={strings.brandingColorBalance}>
      <div className="flex h-14 overflow-hidden rounded-xl border border-default bg-surface shadow-sm" aria-hidden="true">
        <span className="basis-[70%] bg-surface" />
        <span className="basis-[20%]" style={{ backgroundColor: secondary }} />
        <span className="basis-[10%]" style={{ backgroundColor: primary }} />
      </div>
      <div className="mt-4 grid grid-cols-3 gap-2 sm:gap-3">
        {roles.map((role) => (
          <div key={role.label} className="min-w-0 rounded-xl border border-subtle bg-surface px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span
                className="size-3 shrink-0 rounded-full border border-black/10"
                style={{ backgroundColor: role.color }}
              />
              <strong className="truncate text-xs font-semibold text-primary">{role.label}</strong>
            </div>
            <span className="mt-1 block pl-5 text-sm font-semibold tabular-nums text-secondary">{role.ratio}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
