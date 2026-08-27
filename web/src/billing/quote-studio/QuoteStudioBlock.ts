export type DividerThickness = "fine" | "medium" | "bold";
export type DividerStyle = "solid" | "dashed" | "dotted";
export type DividerWidth = 25 | 50 | 75 | 100;

export type QuoteStudioBlock =
  | { id: string; kind: "text"; heading: string; body: string }
  | { id: string; kind: "heading"; level: 1 | 2 | 3; text: string }
  | { id: string; kind: "paragraph"; text: string }
  | { id: string; kind: "quote"; text: string; attribution: string }
  | {
      id: string;
      kind: "list";
      ordered: boolean;
      items: string;
      columns?: 1 | 2 | 3;
    }
  | {
      id: string;
      kind: "divider";
      thickness?: DividerThickness;
      style?: DividerStyle;
      width?: DividerWidth;
      color?: string;
    }
  | {
      id: string;
      kind: "image";
      src: string;
      caption: string;
      body?: string;
      placement?: "full" | "left" | "right";
      columnRatio?: "33-67" | "40-60" | "50-50" | "60-40" | "67-33";
      aspect?: "natural" | "landscape" | "square";
      fit?: "cover" | "contain";
      zoom?: 50 | 75 | 100 | 125 | 150 | 175 | 200;
    }
  | {
      id: string;
      kind: "pricing";
      rowKeys?: string[];
      showSubtotal?: boolean;
      title?: string;
    }
  | {
      id: string;
      kind: "table";
      columns: Array<{ id: string; label: string }>;
      rows: Array<{ id: string; cells: Record<string, string> }>;
    };

export type DividerBlock = Extract<QuoteStudioBlock, { kind: "divider" }>;
export type ImageBlock = Extract<QuoteStudioBlock, { kind: "image" }>;
export type GeneralTable = Extract<QuoteStudioBlock, { kind: "table" }>;
