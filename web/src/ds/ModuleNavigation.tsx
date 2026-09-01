import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cx } from "./cx";

const itemBase =
  "relative inline-flex min-h-11 shrink-0 cursor-pointer items-center gap-2.5 rounded-xl border-0 !px-4 !py-2.5 !text-sm !no-underline transition-colors [&_svg]:!size-4 hover:!no-underline focus-visible:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/30";

export function moduleNavigationItemClassName(selected: boolean) {
  return `${itemBase} ${
    selected
      ? "!bg-accent-soft !font-semibold !text-accent"
      : "!bg-transparent !font-medium !text-secondary hover:!bg-raised hover:!text-primary"
  }`;
}

export interface ModuleNavigationProps extends Omit<
  ComponentPropsWithoutRef<"nav">,
  "aria-label"
> {
  label: string;
  children: ReactNode;
}

/** Canonical horizontally scrolling navigation for sibling module views. */
export function ModuleNavigation({
  label,
  children,
  className,
  ...props
}: ModuleNavigationProps) {
  return (
    <nav
      aria-label={label}
      className={cx("flex min-w-0 gap-2 overflow-x-auto", className)}
      {...props}
    >
      {children}
    </nav>
  );
}
