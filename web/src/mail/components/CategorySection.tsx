// The sidebar "Categories" section: the account's colored labels, each a filter
// (click to show only that category's messages; click again to clear). A "+"
// adds one inline; right-click renames, recolors, or deletes it. Sibling to the
// folder list — categories are labels, not folders, so they live apart.
import { useState } from "react";
import type { KeyboardEvent } from "react";
import { MoreHorizontal, Pencil, Plus, Tag, Tags, Trash2 } from "lucide-react";

import type { Category } from "../../jmap";
import { cx } from "../../ds";
import { strings } from "../../i18n";
import styles from "./CategorySection.module.css";

/** Default palette for new categories (warm-workshop hues + universals). */
const LABEL_COLORS = [
  "#5b8a72", "#3f7cac", "#7b6cae", "#c07a3e",
  "#c0603e", "#b03a4b", "#4c9a8f", "#8a8f3a",
];

interface CategorySectionProps {
  categories: Category[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onCreate: (name: string, color: string | null) => void;
  onUpdate: (id: string, name: string, color: string | null) => void;
  onDelete: (category: Category) => void;
  /** Whether the active account's labels may be managed — false for a read-only
   * shared mailbox, which hides create/rename/delete affordances. */
  canManage: boolean;
}

export function CategorySection({
  categories,
  selectedId,
  onSelect,
  onCreate,
  onUpdate,
  onDelete,
  canManage,
}: CategorySectionProps) {
  const [menu, setMenu] = useState<{ cat: Category; x: number; y: number } | null>(null);
  const [editing, setEditing] = useState<{ id: string; value: string } | null>(null);
  const [creating, setCreating] = useState<string | null>(null);

  function commitCreate() {
    if (creating !== null && creating.trim().length > 0) {
      // A fresh category gets the next palette color, cycling by count.
      onCreate(creating.trim(), LABEL_COLORS[categories.length % LABEL_COLORS.length] ?? null);
    }
    setCreating(null);
  }
  function commitRename() {
    const cat = categories.find((c) => c.id === editing?.id);
    if (editing !== null && cat !== undefined && editing.value.trim().length > 0) {
      onUpdate(cat.id, editing.value.trim(), cat.color);
    }
    setEditing(null);
  }
  function onKey(e: KeyboardEvent<HTMLInputElement>, commit: () => void, cancel: () => void) {
    if (e.key === "Enter") commit();
    else if (e.key === "Escape") cancel();
  }

  return (
    <div className={styles.group}>
      <div className={styles.head}>
        <Tags size={14} className={styles.headIcon} />
        <h2 className={styles.heading}>{strings.categories}</h2>
        {canManage && (
          <button
            type="button"
            className={styles.add}
            onClick={() => setCreating("")}
            title={strings.categoryNew}
            aria-label={strings.categoryNew}
          >
            <Plus size={15} />
          </button>
        )}
      </div>

      {categories.map((c) =>
        editing?.id === c.id ? (
          <div key={c.id} className={styles.item}>
            <span className={styles.dot} style={c.color !== null ? { background: c.color } : undefined} />
            <input
              className={styles.rename}
              value={editing.value}
              autoFocus
              onChange={(e) => setEditing({ id: c.id, value: e.target.value })}
              onBlur={commitRename}
              onKeyDown={(e) => onKey(e, commitRename, () => setEditing(null))}
              aria-label={strings.categoryRename}
            />
          </div>
        ) : (
          <div key={c.id} className={styles.rowWrap}>
            <button
              type="button"
              className={cx(styles.item, selectedId === c.id && styles.active)}
              onClick={() => onSelect(selectedId === c.id ? null : c.id)}
              aria-pressed={selectedId === c.id}
              title={c.name}
              onContextMenu={
                canManage
                  ? (e) => {
                      e.preventDefault();
                      setMenu({ cat: c, x: e.clientX, y: e.clientY });
                    }
                  : undefined
              }
            >
              <span
                className={styles.dot}
                style={c.color !== null ? { background: c.color } : undefined}
                aria-hidden
              />
              <span className={styles.name}>{c.name}</span>
            </button>
            {canManage && (
              <button
                type="button"
                className={styles.kebab}
                aria-label={strings.categoryActions(c.name)}
                title={strings.categoryActions(c.name)}
                onClick={(e) => {
                  e.stopPropagation();
                  const r = e.currentTarget.getBoundingClientRect();
                  setMenu({ cat: c, x: r.right, y: r.bottom });
                }}
              >
                <MoreHorizontal size={15} />
              </button>
            )}
          </div>
        ),
      )}

      {creating !== null && (
        <div className={styles.item}>
          <Tag size={14} className={styles.newIcon} aria-hidden />
          <input
            className={styles.rename}
            value={creating}
            autoFocus
            placeholder={strings.categoryNamePlaceholder}
            onChange={(e) => setCreating(e.target.value)}
            onBlur={commitCreate}
            onKeyDown={(e) => onKey(e, commitCreate, () => setCreating(null))}
            aria-label={strings.categoryNew}
          />
        </div>
      )}

      {menu !== null && (
        <>
          <button
            type="button"
            className={styles.scrim}
            aria-hidden
            tabIndex={-1}
            onClick={() => setMenu(null)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu(null);
            }}
          />
          <div
            className={styles.palette}
            role="menu"
            aria-label={menu.cat.name}
            style={{ left: Math.min(menu.x, window.innerWidth - 200), top: menu.y }}
          >
            <button
              type="button"
              className={`${styles.menuItem} hover:!bg-accent-soft hover:!text-accent`}
              onClick={() => {
                setEditing({ id: menu.cat.id, value: menu.cat.name });
                setMenu(null);
              }}
            >
              <Pencil size={14} />
              {strings.categoryRename}
            </button>
            <div className={styles.divider} />
            <span className={styles.paletteHead}>{strings.labelColor}</span>
            <div className={styles.swatches}>
              {LABEL_COLORS.map((color) => (
                <button
                  key={color}
                  type="button"
                  className={styles.swatch}
                  style={{ background: color }}
                  aria-label={color}
                  onClick={() => {
                    onUpdate(menu.cat.id, menu.cat.name, color);
                    setMenu(null);
                  }}
                />
              ))}
            </div>
            <button
              type="button"
              className={styles.clearColor}
              onClick={() => {
                onUpdate(menu.cat.id, menu.cat.name, null);
                setMenu(null);
              }}
            >
              {strings.labelColorClear}
            </button>
            <div className={styles.divider} />
            <button
              type="button"
              className={cx(styles.menuItem, styles.menuDanger)}
              onClick={() => {
                onDelete(menu.cat);
                setMenu(null);
              }}
            >
              <Trash2 size={14} />
              {strings.categoryDelete}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
