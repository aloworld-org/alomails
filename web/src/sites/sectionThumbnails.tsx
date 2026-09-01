// The schematic thumbnails the palette draws each section type by: not
// screenshots, and not the tenant's content — the *shape* a stranger
// recognizes a block from, in one small line drawing. The tile's own text
// names it, and the tile's preview shows what it would really look like with
// the tenant's own content (S3.01d), so these are decorative by construction.
import type { ReactNode } from "react";

import type { SectionKind } from "./sections";

/** The schematic, one per section type. */
export function sectionThumbnail(kind: SectionKind): ReactNode {
  switch (kind) {
    case "nav":
      return (
        <>
          <rect x="2" y="4" width="60" height="8" rx="2" opacity="0.25" />
          <circle cx="8" cy="8" r="2.5" />
          <rect x="36" y="6.5" width="8" height="3" rx="1.5" />
          <rect x="47" y="5" width="13" height="6" rx="3" opacity="0.7" />
        </>
      );
    case "hero":
      return (
        <>
          <rect x="12" y="8" width="40" height="6" rx="2" />
          <rect x="18" y="18" width="28" height="3" rx="1.5" opacity="0.45" />
          <rect x="24" y="26" width="16" height="7" rx="3.5" opacity="0.7" />
        </>
      );
    case "features":
      return (
        <>
          {[4, 24, 44].map((x) => (
            <g key={x}>
              <circle cx={x + 8} cy="10" r="4" opacity="0.7" />
              <rect x={x} y="18" width="16" height="3" rx="1.5" />
              <rect x={x} y="24" width="16" height="2" rx="1" opacity="0.45" />
              <rect x={x} y="28" width="12" height="2" rx="1" opacity="0.45" />
            </g>
          ))}
        </>
      );
    case "text_image":
      return (
        <>
          <rect x="2" y="6" width="28" height="28" rx="2" opacity="0.35" />
          <circle cx="11" cy="16" r="4" opacity="0.7" />
          <path d="M4 30 L14 20 L20 26 L26 20 L30 24 L30 32 L4 32 Z" opacity="0.7" />
          <rect x="36" y="10" width="24" height="4" rx="2" />
          <rect x="36" y="18" width="24" height="2" rx="1" opacity="0.45" />
          <rect x="36" y="22" width="24" height="2" rx="1" opacity="0.45" />
          <rect x="36" y="26" width="16" height="2" rx="1" opacity="0.45" />
        </>
      );
    case "gallery":
      return (
        <>
          {[2, 23, 44].map((x) =>
            [4, 22].map((y) => (
              <rect key={`${x}-${y}`} x={x} y={y} width="18" height="14" rx="2" opacity="0.4" />
            )),
          )}
        </>
      );
    case "testimonials":
      return (
        <>
          <path d="M8 8 q-4 0 -4 5 v3 h6 v-6 q0 -2 -2 -2 Z" opacity="0.7" />
          <rect x="14" y="8" width="46" height="3" rx="1.5" opacity="0.45" />
          <rect x="14" y="14" width="38" height="3" rx="1.5" opacity="0.45" />
          <circle cx="10" cy="30" r="4" opacity="0.7" />
          <rect x="18" y="28" width="20" height="3" rx="1.5" />
        </>
      );
    case "pricing":
      return (
        <>
          <rect x="3" y="10" width="17" height="24" rx="2" opacity="0.3" />
          <rect x="23" y="5" width="18" height="30" rx="2" opacity="0.6" />
          <rect x="44" y="10" width="17" height="24" rx="2" opacity="0.3" />
        </>
      );
    case "team":
      return (
        <>
          {[10, 32, 54].map((x) => (
            <g key={x}>
              <circle cx={x} cy="12" r="6" opacity="0.6" />
              <rect x={x - 7} y="22" width="14" height="3" rx="1.5" />
              <rect x={x - 5} y="28" width="10" height="2" rx="1" opacity="0.45" />
            </g>
          ))}
        </>
      );
    case "faq":
      return (
        <>
          {[4, 16, 28].map((y) => (
            <g key={y}>
              <rect x="2" y={y} width="60" height="8" rx="2" opacity="0.25" />
              <rect x="6" y={y + 3} width="30" height="2" rx="1" />
              <path d={`M54 ${y + 2.5} l3 3 l3 -3`} fill="none" stroke="currentColor" />
            </g>
          ))}
        </>
      );
    case "cta":
      return (
        <>
          <rect x="2" y="10" width="60" height="20" rx="3" opacity="0.25" />
          <rect x="8" y="16" width="26" height="4" rx="2" />
          <rect x="42" y="15" width="14" height="7" rx="3.5" opacity="0.7" />
        </>
      );
    case "contact_form":
      return (
        <>
          <rect x="8" y="4" width="48" height="6" rx="2" opacity="0.35" />
          <rect x="8" y="13" width="48" height="6" rx="2" opacity="0.35" />
          <rect x="8" y="22" width="48" height="8" rx="2" opacity="0.35" />
          <rect x="40" y="33" width="16" height="5" rx="2.5" opacity="0.7" />
        </>
      );
    case "collection":
      return (
        <>
          {[3, 24, 45].map((x) => (
            <g key={x}>
              <rect x={x} y="6" width="16" height="28" rx="2" opacity="0.3" />
              <rect x={x + 3} y="10" width="10" height="8" rx="1" opacity="0.7" />
              <rect x={x + 3} y="22" width="10" height="2" rx="1" />
              <rect x={x + 3} y="27" width="7" height="2" rx="1" opacity="0.5" />
            </g>
          ))}
        </>
      );
    case "catalog":
      return (
        <>
          {[6, 22].map((y) => (
            <g key={y}>
              <rect x="4" y={y} width="12" height="12" rx="2" opacity="0.55" />
              <rect x="20" y={y + 2} width="22" height="3" rx="1.5" />
              <rect x="20" y={y + 8} width="14" height="2" rx="1" opacity="0.45" />
              <rect x="50" y={y + 3} width="10" height="4" rx="2" opacity="0.7" />
            </g>
          ))}
        </>
      );
    case "booking":
      return (
        <>
          <rect x="4" y="6" width="26" height="28" rx="2" opacity="0.3" />
          <rect x="8" y="10" width="18" height="3" rx="1.5" />
          {[16, 22, 28].map((y) => (
            <rect key={y} x="8" y={y} width="18" height="3" rx="1.5" opacity="0.5" />
          ))}
          <rect x="36" y="10" width="24" height="4" rx="2" opacity="0.7" />
          <rect x="36" y="19" width="24" height="3" rx="1.5" opacity="0.45" />
          <rect x="36" y="27" width="14" height="6" rx="3" opacity="0.7" />
        </>
      );
    case "tickets":
      // Two ticket stubs, perforation line and all: what the block sells is
      // an admission, and the date line under each name says it is dated.
      return (
        <>
          {[6, 22].map((y) => (
            <g key={y}>
              <rect x="4" y={y} width="42" height="12" rx="2" opacity="0.3" />
              <rect x="8" y={y + 3} width="22" height="3" rx="1.5" />
              <rect x="8" y={y + 8} width="14" height="2" rx="1" opacity="0.5" />
              <rect x="36" y={y + 2} width="1.5" height="8" rx="0.75" opacity="0.5" />
              <rect x="50" y={y + 4} width="10" height="4" rx="2" opacity="0.7" />
            </g>
          ))}
        </>
      );
    case "shop":
      // Two product cards, each a box with a picture, a name line and a price
      // chip: what the block sells sits on a shelf, undated.
      return (
        <>
          {[4, 34].map((x) => (
            <g key={x}>
              <rect x={x} y="6" width="26" height="28" rx="2" opacity="0.3" />
              <rect x={x + 3} y="9" width="20" height="11" rx="1.5" opacity="0.5" />
              <rect x={x + 3} y="23" width="14" height="3" rx="1.5" />
              <rect x={x + 3} y="28" width="8" height="3" rx="1.5" opacity="0.7" />
            </g>
          ))}
        </>
      );
    case "custom_code":
      // A framed box with brackets in it: the block is code, and the frame
      // around it is the point rather than decoration.
      return (
        <>
          <rect
            x="6"
            y="6"
            width="52"
            height="28"
            rx="3"
            fill="none"
            stroke="currentColor"
            strokeDasharray="4 3"
            opacity="0.6"
          />
          <path
            d="M24 15 L18 20 L24 25 M40 15 L46 20 L40 25"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          />
          <rect x="30" y="13" width="4" height="14" rx="2" opacity="0.45" />
        </>
      );
    case "transition":
      return (
        <>
          <rect x="4" y="5" width="56" height="9" rx="3" opacity="0.22" />
          <path d="M32 16 V28 M27 23 L32 28 L37 23" fill="none" stroke="currentColor" strokeWidth="2" />
          <rect x="4" y="30" width="56" height="7" rx="3" opacity="0.65" />
        </>
      );
    case "footer":
      return (
        <>
          <rect x="2" y="26" width="60" height="12" rx="2" opacity="0.25" />
          <rect x="6" y="31" width="16" height="2" rx="1" />
          <rect x="34" y="31" width="8" height="2" rx="1" opacity="0.45" />
          <rect x="45" y="31" width="8" height="2" rx="1" opacity="0.45" />
        </>
      );
  }
}
