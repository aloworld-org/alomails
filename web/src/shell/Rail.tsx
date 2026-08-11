// The left rail — the constant of the one-product frame (Figma app shell).
// Top: the mark and ＋New. Middle: one labelled item per registered module,
// the active one highlighted. Bottom: ✦AI and the account menu. It never
// scrolls and never changes between modules; only the panel to its right does.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Check,
  Grip,
  GripVertical,
  Pencil,
  Plus,
  Sparkles,
  X,
} from "lucide-react";
import { NavLink } from "react-router-dom";

import { strings } from "../i18n";
import { cx } from "../ds";
import { surface } from "../product";
import { mostUsedApps } from "./appUsage";
import { Logo } from "./Logo";
import { UserMenu } from "./UserMenu";
import styles from "./Rail.module.css";

interface RailProps {
  /** ✦AI action (assistant panel — placeholder until the AI layer). */
  onAskAi: () => void;
}

const FAVORITES_KEY = "alo-rail-favorites";

export function Rail({ onAskAi }: RailProps) {
  const apps = surface.modules.filter((module) => module.id !== "home");
  const home = surface.modules.find((module) => module.id === "home");
  // What this person actually opens, strongest first. Only when they have
  // used nothing yet do we fall back to declaration order, which is a guess
  // about somebody made before they did anything.
  const used = mostUsedApps(6).filter((id) => apps.some((a) => a.id === id));
  const defaultFavorites =
    used.length > 0
      ? [
          ...used,
          ...apps.map((m) => m.id).filter((id) => !used.includes(id)),
        ].slice(0, 6)
      : apps.slice(0, 6).map((module) => module.id);
  const [favorites, setFavorites] = useState<string[]>(() => {
    try {
      const saved = JSON.parse(
        window.localStorage.getItem(FAVORITES_KEY) ?? "[]",
      ) as unknown;
      if (Array.isArray(saved)) {
        const valid = saved.filter(
          (id): id is string =>
            typeof id === "string" && apps.some((app) => app.id === id),
        );
        if (valid.length > 0) return [...new Set(valid)].slice(0, 6);
      }
    } catch {
      // A corrupt preference should never prevent navigation from rendering.
    }
    return defaultFavorites;
  });
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(favorites);
  const draggedRef = useRef<string | null>(null);
  const launcherTriggerRef = useRef<HTMLLIElement>(null);
  const launcherPanelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !launcherTriggerRef.current?.contains(target) &&
        !launcherPanelRef.current?.contains(target)
      ) {
        setOpen(false);
        setEditing(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        setEditing(false);
      }
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const favoriteModules = favorites.flatMap((id) => {
    const module = apps.find((app) => app.id === id);
    return module === undefined ? [] : [module];
  });
  const draftModules = draft.flatMap((id) => {
    const module = apps.find((app) => app.id === id);
    return module === undefined ? [] : [module];
  });

  const saveFavorites = () => {
    setFavorites(draft);
    window.localStorage.setItem(FAVORITES_KEY, JSON.stringify(draft));
    setEditing(false);
  };

  const toggleFavorite = (id: string) => {
    setDraft((current) =>
      current.includes(id)
        ? current.filter((favorite) => favorite !== id)
        : current.length < 6
          ? [...current, id]
          : current,
    );
  };

  const moveFavorite = (target: string) => {
    const dragged = draggedRef.current;
    if (dragged === null || dragged === target) return;
    setDraft((current) => {
      const from = current.indexOf(dragged);
      const to = current.indexOf(target);
      if (from < 0 || to < 0) return current;
      const next = [...current];
      const [moved] = next.splice(from, 1);
      if (moved === undefined) return current;
      next.splice(to, 0, moved);
      return next;
    });
    draggedRef.current = null;
  };

  return (
    <nav className={styles.rail} aria-label={strings.appName}>
      <div className={styles.top}>
        <NavLink
          to="/mail"
          className={cx(styles.logoLink)}
          aria-label={strings.appName}
        >
          <Logo size={40} />
        </NavLink>
      </div>

      <ul className={styles.modules}>
        {home !== undefined && (
          <li>
            <NavLink
              to={home.path}
              className={({ isActive }) =>
                cx(styles.item, isActive && styles.active)
              }
              title={home.label}
            >
              <home.Icon strokeWidth={1.75} />
              <span className={styles.label}>{home.label}</span>
            </NavLink>
          </li>
        )}
        <li ref={launcherTriggerRef} className={styles.launcherAnchor}>
          <button
            type="button"
            className={cx(styles.item, open && styles.active)}
            onClick={() => {
              setOpen((current) => !current);
              setEditing(false);
            }}
            aria-expanded={open}
            aria-haspopup="dialog"
            title={strings.appLauncher}
          >
            <Grip strokeWidth={2} />
            <span className={styles.label}>{strings.appLauncher}</span>
          </button>
          {open &&
            createPortal(
              <div
                ref={launcherPanelRef}
                className={styles.launcher}
                role="dialog"
                aria-label={strings.appLauncher}
              >
                <div className={styles.launcherHead}>
                  {editing ? (
                    <>
                      <button
                        type="button"
                        className={styles.launcherSecondary}
                        onClick={() => {
                          setDraft(favorites);
                          setEditing(false);
                        }}
                      >
                        {strings.appLauncherCancel}
                      </button>
                      <strong>{strings.appLauncherDragHint}</strong>
                      <button
                        type="button"
                        className={styles.launcherPrimary}
                        onClick={saveFavorites}
                        disabled={draft.length !== 6}
                      >
                        <Check size={15} />
                        {strings.appLauncherDone}
                      </button>
                    </>
                  ) : (
                    <>
                      <strong>{strings.appLauncherFavorites}</strong>
                      <button
                        type="button"
                        className={styles.launcherEdit}
                        onClick={() => {
                          setDraft(favorites);
                          setEditing(true);
                        }}
                        aria-label={strings.appLauncherEdit}
                        title={strings.appLauncherEdit}
                      >
                        <Pencil size={17} />
                      </button>
                    </>
                  )}
                </div>
                <div className={styles.launcherScroll}>
                  <div className={styles.favoriteCard}>
                    <div className={styles.appGrid}>
                      {(editing ? draftModules : favoriteModules).map((app) =>
                        editing ? (
                          <button
                            key={app.id}
                            type="button"
                            className={styles.appTile}
                            draggable
                            onDragStart={() => {
                              draggedRef.current = app.id;
                            }}
                            onDragEnd={() => {
                              draggedRef.current = null;
                            }}
                            onDragOver={(event) => event.preventDefault()}
                            onDrop={() => moveFavorite(app.id)}
                            onClick={() => toggleFavorite(app.id)}
                            title={strings.appLauncherRemoveFavorite}
                          >
                            <GripVertical
                              className={styles.dragHandle}
                              size={14}
                            />
                            <app.Icon />
                            <span>{app.label}</span>
                            <X className={styles.removeFavorite} size={13} />
                          </button>
                        ) : (
                          <NavLink
                            key={app.id}
                            to={app.path}
                            className={cx(styles.appTile)}
                            onClick={() => setOpen(false)}
                          >
                            <app.Icon />
                            <span>{app.label}</span>
                          </NavLink>
                        ),
                      )}
                    </div>
                  </div>
                  <h3>{strings.appLauncherAll}</h3>
                  <div className={styles.appGrid}>
                    {apps.map((app) =>
                      editing ? (
                        <button
                          key={app.id}
                          type="button"
                          className={cx(
                            styles.appTile,
                            draft.includes(app.id) && styles.appTileFavorite,
                          )}
                          onClick={() => toggleFavorite(app.id)}
                          title={
                            draft.includes(app.id)
                              ? strings.appLauncherRemoveFavorite
                              : strings.appLauncherAddFavorite
                          }
                        >
                          <app.Icon />
                          <span>{app.label}</span>
                          {draft.includes(app.id) ? (
                            <Check className={styles.favoriteMark} size={14} />
                          ) : (
                            <Plus className={styles.favoriteMark} size={14} />
                          )}
                        </button>
                      ) : (
                        <NavLink
                          key={app.id}
                          to={app.path}
                          className={cx(styles.appTile)}
                          onClick={() => setOpen(false)}
                        >
                          <app.Icon />
                          <span>{app.label}</span>
                        </NavLink>
                      ),
                    )}
                  </div>
                </div>
              </div>,
              document.body,
            )}
        </li>
        {favoriteModules.map((m) => (
          <li key={m.id}>
            <NavLink
              to={m.path}
              className={({ isActive }) =>
                cx(styles.item, isActive && styles.active)
              }
              title={m.label}
            >
              <m.Icon strokeWidth={1.75} />
              <span className={styles.label}>{m.label}</span>
            </NavLink>
          </li>
        ))}
      </ul>

      <div className={styles.bottom}>
        {/* Whatever this product says must be visible from every module — the
            running timer, in the workspace. Each renders nothing when it has
            nothing to say; the rail knows what none of them are about. */}
        {(surface.railWidgets ?? []).map((widget) => (
          <widget.Widget key={widget.id} />
        ))}
        <button type="button" className={styles.item} onClick={onAskAi}>
          <Sparkles strokeWidth={1.75} />
          <span className={styles.label}>{strings.moduleAi}</span>
        </button>
        <UserMenu />
      </div>
    </nav>
  );
}
