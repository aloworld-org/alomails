import { NavLink } from "react-router-dom";

import { cx } from "../ds";
import type { ProductModule } from "../product";

interface AppTileProps {
  app: ProductModule;
  onSelect: () => void;
}

export function AppTile({ app, onSelect }: AppTileProps) {
  return (
    <NavLink
      to={app.path}
      className={({ isActive }) =>
        cx(
          "group flex min-h-24 min-w-0 flex-col items-center rounded-2xl border border-transparent px-2 py-2.5 text-center text-[#102A43] transition-colors duration-150 hover:bg-[#FAF7F2] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15",
          isActive && "border-[#E76F51]/20 bg-[#E76F51]/10",
        )
      }
      onClick={onSelect}
    >
      <span className="grid size-12 shrink-0 place-items-center rounded-xl border border-[#CBD5E1]/55 bg-[#FAF7F2] text-[#102A43] shadow-[0_1px_2px_rgba(16,42,67,0.04)] transition-colors duration-150 group-hover:border-[#E76F51]/25 group-hover:text-[#E76F51] group-aria-[current=page]:border-[#E76F51]/25 group-aria-[current=page]:bg-white group-aria-[current=page]:text-[#E76F51]">
        <app.Icon className="size-5" strokeWidth={1.75} aria-hidden="true" />
      </span>
      <span className="mt-2 max-w-full text-sm font-medium leading-5 [overflow-wrap:anywhere]">
        {app.label}
      </span>
    </NavLink>
  );
}
